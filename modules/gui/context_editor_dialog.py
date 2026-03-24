"""Context editor dialog for editing context (text and images) and clipboard."""

import logging
from dataclasses import dataclass

from PySide6.QtCore import QEvent, Qt, QTimer, Signal
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from core.context_manager import ContextItem, ContextItemType, ContextManager
from modules.gui.shared.base_dialog import BaseDialog
from modules.gui.shared.theme import (
    DEFAULT_WRAPPED_HEIGHT,
    DIALOG_CONTENT_MARGINS,
    SCROLL_CONTENT_MARGINS,
    SCROLL_CONTENT_SPACING,
    TEXT_CHANGE_DEBOUNCE_MS,
    apply_section_size_policy,
    apply_wrap_state,
    create_singleton_dialog_manager,
)
from modules.gui.shared.undo_redo import perform_redo, perform_undo
from modules.gui.shared.widgets import (
    CollapsibleSectionHeader,
    ImageChipWidget,
    create_text_edit,
)
from modules.utils.notification_config import is_notification_enabled

logger = logging.getLogger(__name__)

# Singleton dialog manager
_show_dialog = create_singleton_dialog_manager()


@dataclass
class ContextState:
    """Snapshot of context editor state for undo/redo."""

    images: list[ContextItem]
    text: str


@dataclass
class ClipboardState:
    """Snapshot of clipboard editor state for undo/redo."""

    text: str
    image: tuple[str, str] | None  # (base64, media_type)


def show_context_editor(
    context_manager: ContextManager,
    clipboard_manager,
    notification_manager=None,
):
    """Show the context editor dialog. If already open, bring to front."""
    _show_dialog(
        "context_editor",
        lambda: ContextEditorDialog(
            context_manager,
            clipboard_manager,
            notification_manager,
        ),
    )


class ContextEditorDialog(BaseDialog):
    """Dialog for editing context (text and images) and clipboard."""

    STATE_KEY = "context_editor_dialog"
    context_saved = Signal()

    def __init__(
        self,
        context_manager: ContextManager,
        clipboard_manager,
        notification_manager=None,
        parent=None,
    ):
        super().__init__(parent)
        self.context_manager = context_manager
        self.clipboard_manager = clipboard_manager
        self.notification_manager = notification_manager

        # Working state
        self._current_images: list[ContextItem] = []
        self._image_chips: list[ImageChipWidget] = []
        self._clipboard_image: tuple[str, str] | None = None  # (base64, media_type)
        self._clipboard_image_chip: ImageChipWidget | None = None

        # Separate undo/redo stacks for context and clipboard
        self._context_undo_stack: list[ContextState] = []
        self._context_redo_stack: list[ContextState] = []
        self._clipboard_undo_stack: list[ClipboardState] = []
        self._clipboard_redo_stack: list[ClipboardState] = []

        self.setWindowTitle("Context Editor")

        self._setup_ui()
        self.apply_dialog_styles()
        self._load_context()
        self._restore_ui_state()

        # Install event filter to intercept Ctrl+V on text_edit and clipboard_edit
        self.text_edit.installEventFilter(self)
        self.clipboard_edit.installEventFilter(self)

    def _setup_ui(self):
        """Setup the dialog UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(*DIALOG_CONTENT_MARGINS)
        layout.setSpacing(SCROLL_CONTENT_SPACING)

        # Images section (not in scroll area - fixed height at top)
        self.images_section = self._create_images_section()
        layout.addWidget(self.images_section)

        # Scroll area for sections
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.scroll_area.setFrameShape(QFrame.NoFrame)

        # Container for sections
        self.sections_container = QWidget()
        self.sections_layout = QVBoxLayout(self.sections_container)
        self.sections_layout.setContentsMargins(*SCROLL_CONTENT_MARGINS)
        self.sections_layout.setSpacing(SCROLL_CONTENT_SPACING)

        # Context section
        self.context_section = self._create_context_section()
        self.sections_layout.addWidget(self.context_section)

        # Clipboard section
        self.clipboard_section = self._create_clipboard_section()
        self.sections_layout.addWidget(self.clipboard_section)

        self.scroll_area.setWidget(self.sections_container)
        layout.addWidget(self.scroll_area, 1)  # Stretch to fill

        # Button bar (wrapped in QWidget for consistent positioning)
        button_widget = QWidget()
        button_bar = QHBoxLayout(button_widget)
        button_bar.setContentsMargins(0, 0, 12, 0)  # 12px right margin for alignment
        button_bar.addStretch()

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        button_bar.addWidget(cancel_btn)

        save_btn = QPushButton("Save")
        save_btn.clicked.connect(self._on_save_clicked)
        save_btn.setDefault(True)
        button_bar.addWidget(save_btn)

        layout.addWidget(button_widget)

        # Track text changes for undo
        self._last_text = ""
        self._last_clipboard_text = ""
        self._text_change_timer = QTimer()
        self._text_change_timer.setSingleShot(True)
        self._text_change_timer.setInterval(TEXT_CHANGE_DEBOUNCE_MS)
        self._text_change_timer.timeout.connect(self._save_text_state)

    def _create_images_section(self) -> QWidget:
        """Create the images row (no header, just chips)."""
        # Just the images container - no header
        self.images_content = QWidget()
        self.images_content.setStyleSheet("background: transparent;")
        self.images_layout = QHBoxLayout(self.images_content)
        self.images_layout.setContentsMargins(0, 0, 0, 4)
        self.images_layout.setSpacing(6)
        self.images_layout.addStretch()

        return self.images_content

    def _create_context_section(self) -> QWidget:
        """Create the collapsible context text section."""
        container = QWidget()
        apply_section_size_policy(container, expanding=False)
        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(4)

        # Header with collapse toggle, wrap button, hint, undo/redo, and save button
        self.context_header = CollapsibleSectionHeader(
            "Context",
            show_undo_redo=True,
            show_wrap_button=True,
            hint_text="(Paste image: Ctrl+V)",
        )
        self.context_header.toggle_requested.connect(self._toggle_context_section)
        self.context_header.wrap_requested.connect(self._toggle_context_wrap)
        self.context_header.save_requested.connect(self._save_context_only)
        self.context_header.undo_requested.connect(self._undo_context)
        self.context_header.redo_requested.connect(self._redo_context)
        section_layout.addWidget(self.context_header)

        # Text edit area
        self.text_edit = create_text_edit(placeholder="Context content...", min_height=0)
        self.text_edit.textChanged.connect(self._on_text_changed)
        self.text_edit.setMaximumHeight(DEFAULT_WRAPPED_HEIGHT)  # Default wrapped height
        section_layout.addWidget(self.text_edit)

        return container

    def _create_clipboard_section(self) -> QWidget:
        """Create the collapsible clipboard text section."""
        container = QWidget()
        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(4)

        # Header with collapse toggle, wrap button, undo/redo, and save button
        self.clipboard_header = CollapsibleSectionHeader(
            "Clipboard",
            show_undo_redo=True,
            show_wrap_button=True,
        )
        self.clipboard_header.toggle_requested.connect(self._toggle_clipboard_section)
        self.clipboard_header.wrap_requested.connect(self._toggle_clipboard_wrap)
        self.clipboard_header.save_requested.connect(self._save_clipboard_only)
        self.clipboard_header.undo_requested.connect(self._undo_clipboard)
        self.clipboard_header.redo_requested.connect(self._redo_clipboard)
        section_layout.addWidget(self.clipboard_header)

        # Image container (shown when clipboard has image) - fixed height, not stretched
        self.clipboard_image_container = QWidget()
        self.clipboard_image_container.setStyleSheet("background: transparent;")
        self.clipboard_image_layout = QHBoxLayout(self.clipboard_image_container)
        self.clipboard_image_layout.setContentsMargins(0, 4, 0, 4)
        self.clipboard_image_layout.setSpacing(6)
        self.clipboard_image_layout.addStretch()
        section_layout.addWidget(self.clipboard_image_container, 0, Qt.AlignTop)
        self.clipboard_image_container.hide()  # Hidden by default

        # Text edit area (shown when clipboard has text)
        self.clipboard_edit = create_text_edit(placeholder="Clipboard content...", min_height=0)
        self.clipboard_edit.textChanged.connect(self._on_clipboard_text_changed)
        section_layout.addWidget(self.clipboard_edit)

        section_layout.addStretch()

        apply_section_size_policy(container, expanding=True, widget=self.clipboard_edit)

        return container

    def _toggle_context_section(self):
        """Toggle context section visibility."""
        self.toggle_section_collapsed(
            "context",
            self.context_header,
            self.text_edit,
            self.context_section,
            expanding=False,
        )

    def _toggle_clipboard_section(self):
        """Toggle clipboard section visibility."""
        content_visible = self.clipboard_edit.isVisible() or self.clipboard_image_container.isVisible()

        if content_visible:
            self.clipboard_edit.hide()
            self.clipboard_image_container.hide()
        else:
            if self._clipboard_image:
                self.clipboard_image_container.show()
            else:
                self.clipboard_edit.show()

        self.clipboard_header.set_collapsed(content_visible)
        if content_visible:
            self.clipboard_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        else:
            self.clipboard_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Expanding)
        self.save_section_state("clipboard_collapsed", content_visible)

    def _toggle_context_wrap(self):
        """Toggle context section wrap state."""
        is_wrapped = self.context_header.is_wrapped()
        new_wrapped = not is_wrapped
        self.context_header.set_wrap_state(new_wrapped)
        apply_wrap_state(self.text_edit, new_wrapped)
        self.save_section_state("context_wrapped", new_wrapped)

    def _toggle_clipboard_wrap(self):
        """Toggle clipboard section wrap state."""
        is_wrapped = self.clipboard_header.is_wrapped()
        new_wrapped = not is_wrapped
        self.clipboard_header.set_wrap_state(new_wrapped)
        apply_wrap_state(self.clipboard_edit, new_wrapped)
        self.save_section_state("clipboard_wrapped", new_wrapped)

    def _restore_ui_state(self):
        """Restore collapsed and wrap states from saved state."""
        self.restore_geometry_from_state()

        self.restore_section_collapsed("context", self.context_header, self.text_edit, self.context_section)
        self.restore_section_collapsed(
            "clipboard",
            self.clipboard_header,
            [self.clipboard_edit, self.clipboard_image_container],
            self.clipboard_section,
        )

        context_wrapped = self.get_section_state("context_wrapped", True)
        clipboard_wrapped = self.get_section_state("clipboard_wrapped", True)

        self.context_header.set_wrap_state(context_wrapped)
        apply_wrap_state(self.text_edit, context_wrapped)

        self.clipboard_header.set_wrap_state(clipboard_wrapped)
        apply_wrap_state(self.clipboard_edit, clipboard_wrapped)

    def _save_context_only(self):
        """Save only the context changes."""
        self.context_manager.clear_context()

        # Add images first
        for image_item in self._current_images:
            self.context_manager.append_context_image(
                image_item.data,
                image_item.media_type or "image/png",
            )

        # Add text
        text_content = self.text_edit.toPlainText().strip()
        if text_content:
            self.context_manager.append_context(text_content)

        from modules.gui.shared.icon_confirmation import flash_confirmation

        if self.context_header.save_btn:
            flash_confirmation(self.context_header.save_btn)

    def _save_clipboard_only(self):
        """Save only the clipboard changes."""
        if self._clipboard_image:
            # Copy image to system clipboard
            if self._clipboard_image_chip:
                self._clipboard_image_chip.copy_to_clipboard()
        elif self.clipboard_edit.isVisible():
            # Save text to clipboard
            clipboard_content = self.clipboard_edit.toPlainText()
            self.clipboard_manager.set_content(clipboard_content)

        from modules.gui.shared.icon_confirmation import flash_confirmation

        if self.clipboard_header.save_btn:
            flash_confirmation(self.clipboard_header.save_btn)

    def _load_context(self):
        """Load current context and clipboard into the dialog."""
        items = self.context_manager.get_items()

        # Separate images and text
        self._current_images = [
            ContextItem(
                item_type=item.item_type,
                data=item.data,
                media_type=item.media_type,
            )
            for item in items
            if item.item_type == ContextItemType.IMAGE
        ]

        text_items = [item.content for item in items if item.item_type == ContextItemType.TEXT and item.content]
        text_content = "\n".join(text_items)

        self._rebuild_image_chips()
        self.text_edit.setPlainText(text_content)
        self._last_text = text_content

        # Load clipboard content (check for image first)
        try:
            if self.clipboard_manager.has_image():
                image_data = self.clipboard_manager.get_image_data()
                if image_data:
                    self._clipboard_image = image_data
                    self._rebuild_clipboard_image_chip()
                    self.clipboard_image_container.show()
                    self.clipboard_edit.hide()
                    self._last_clipboard_text = ""
                else:
                    self._load_clipboard_text()
            else:
                self._load_clipboard_text()
        except Exception as e:
            logger.warning(f"Failed to load clipboard content: {e}")
            self._load_clipboard_text()

        # Clear undo/redo stacks
        self._context_undo_stack.clear()
        self._context_redo_stack.clear()
        self._clipboard_undo_stack.clear()
        self._clipboard_redo_stack.clear()
        self._update_undo_redo_buttons()

    def _rebuild_image_chips(self):
        """Rebuild the image chips from current state."""
        # Clear existing chips
        for chip in self._image_chips:
            chip.deleteLater()
        self._image_chips.clear()

        # Remove all items from layout
        while self.images_layout.count():
            self.images_layout.takeAt(0)

        if not self._current_images:
            self.images_section.hide()
            return

        self.images_section.show()

        for idx, item in enumerate(self._current_images):
            chip = ImageChipWidget(
                index=idx,
                image_number=idx + 1,
                image_data=item.data or "",
                media_type=item.media_type or "image/png",
            )
            chip.delete_requested.connect(self._on_image_delete)
            chip.copy_requested.connect(self._on_image_copy)
            self._image_chips.append(chip)
            self.images_layout.addWidget(chip)

        self.images_layout.addStretch()

    def _load_clipboard_text(self):
        """Load text content from clipboard."""
        try:
            clipboard_content = self.clipboard_manager.get_content()
            self.clipboard_edit.setPlainText(clipboard_content or "")
            self._last_clipboard_text = clipboard_content or ""
        except Exception as e:
            logger.warning(f"Failed to load clipboard text: {e}")
            self.clipboard_edit.setPlainText("")
            self._last_clipboard_text = ""
        self._clipboard_image = None
        self.clipboard_image_container.hide()
        self.clipboard_edit.show()

    def _rebuild_clipboard_image_chip(self):
        """Rebuild the clipboard image chip."""
        # Clear existing chip
        if self._clipboard_image_chip:
            self._clipboard_image_chip.deleteLater()
            self._clipboard_image_chip = None

        # Remove all items from layout
        while self.clipboard_image_layout.count():
            self.clipboard_image_layout.takeAt(0)

        if not self._clipboard_image:
            return

        base64_data, media_type = self._clipboard_image
        chip = ImageChipWidget(
            index=0,
            image_number=1,
            image_data=base64_data,
            media_type=media_type,
        )
        chip.delete_requested.connect(self._on_clipboard_image_delete)
        chip.copy_requested.connect(self._on_clipboard_image_copy)
        self._clipboard_image_chip = chip
        self.clipboard_image_layout.addWidget(chip)
        self.clipboard_image_layout.addStretch()

    def _on_clipboard_image_delete(self, index: int):
        """Handle clipboard image delete request."""
        self._save_clipboard_state()
        self._clipboard_image = None
        self._rebuild_clipboard_image_chip()
        self.clipboard_image_container.hide()
        self.clipboard_edit.show()
        self.clipboard_edit.setPlainText("")
        self._last_clipboard_text = ""

    def _on_clipboard_image_copy(self, index: int):
        """Handle clipboard image copy request."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        if self._clipboard_image_chip:
            self._clipboard_image_chip.copy_to_clipboard()
            flash_confirmation(self._clipboard_image_chip.copy_btn)

    # --- Context state management ---

    def _get_context_state(self) -> ContextState:
        """Get current context state."""
        return ContextState(
            images=[
                ContextItem(
                    item_type=item.item_type,
                    data=item.data,
                    media_type=item.media_type,
                )
                for item in self._current_images
            ],
            text=self.text_edit.toPlainText(),
        )

    def _restore_context_state(self, state: ContextState):
        """Restore context state."""
        self._current_images = [
            ContextItem(
                item_type=item.item_type,
                data=item.data,
                media_type=item.media_type,
            )
            for item in state.images
        ]
        self._rebuild_image_chips()

        # Block signal to prevent recursive undo state saving
        self.text_edit.blockSignals(True)
        self.text_edit.setPlainText(state.text)
        self._last_text = state.text
        self.text_edit.blockSignals(False)

    def _save_context_state(self):
        """Save current context state to undo stack."""
        state = self._get_context_state()
        self._context_undo_stack.append(state)
        self._context_redo_stack.clear()
        self._update_undo_redo_buttons()

    def _undo_context(self):
        """Undo last context change."""
        if perform_undo(
            self._context_undo_stack,
            self._context_redo_stack,
            self._get_context_state,
            self._restore_context_state,
        ):
            self._update_undo_redo_buttons()

    def _redo_context(self):
        """Redo last undone context change."""
        if perform_redo(
            self._context_undo_stack,
            self._context_redo_stack,
            self._get_context_state,
            self._restore_context_state,
        ):
            self._update_undo_redo_buttons()

    # --- Clipboard state management ---

    def _get_clipboard_state(self) -> ClipboardState:
        """Get current clipboard state."""
        return ClipboardState(
            text=self.clipboard_edit.toPlainText(),
            image=self._clipboard_image,
        )

    def _restore_clipboard_state(self, state: ClipboardState):
        """Restore clipboard state."""
        self.clipboard_edit.blockSignals(True)
        self.clipboard_edit.setPlainText(state.text)
        self._last_clipboard_text = state.text
        self.clipboard_edit.blockSignals(False)

        # Restore clipboard image state
        self._clipboard_image = state.image
        self._rebuild_clipboard_image_chip()
        if self._clipboard_image:
            self.clipboard_image_container.show()
            self.clipboard_edit.hide()
        else:
            self.clipboard_image_container.hide()
            self.clipboard_edit.show()

    def _save_clipboard_state(self):
        """Save current clipboard state to undo stack."""
        state = self._get_clipboard_state()
        self._clipboard_undo_stack.append(state)
        self._clipboard_redo_stack.clear()
        self._update_undo_redo_buttons()

    def _undo_clipboard(self):
        """Undo last clipboard change."""
        if perform_undo(
            self._clipboard_undo_stack,
            self._clipboard_redo_stack,
            self._get_clipboard_state,
            self._restore_clipboard_state,
        ):
            self._update_undo_redo_buttons()

    def _redo_clipboard(self):
        """Redo last undone clipboard change."""
        if perform_redo(
            self._clipboard_undo_stack,
            self._clipboard_redo_stack,
            self._get_clipboard_state,
            self._restore_clipboard_state,
        ):
            self._update_undo_redo_buttons()

    # --- Common state management ---

    def _update_undo_redo_buttons(self):
        """Update undo/redo button states for both sections."""
        self.context_header.set_undo_redo_enabled(
            len(self._context_undo_stack) > 0,
            len(self._context_redo_stack) > 0,
        )
        self.clipboard_header.set_undo_redo_enabled(
            len(self._clipboard_undo_stack) > 0,
            len(self._clipboard_redo_stack) > 0,
        )

    def _save_text_state(self):
        """Save state if text has significantly changed."""
        current_text = self.text_edit.toPlainText()
        current_clipboard = self.clipboard_edit.toPlainText()

        # Save context state if text changed
        if current_text != self._last_text:
            state = ContextState(
                images=[
                    ContextItem(
                        item_type=item.item_type,
                        data=item.data,
                        media_type=item.media_type,
                    )
                    for item in self._current_images
                ],
                text=self._last_text,
            )
            self._context_undo_stack.append(state)
            self._context_redo_stack.clear()
            self._last_text = current_text

        # Save clipboard state if text changed
        if current_clipboard != self._last_clipboard_text:
            state = ClipboardState(
                text=self._last_clipboard_text,
                image=self._clipboard_image,
            )
            self._clipboard_undo_stack.append(state)
            self._clipboard_redo_stack.clear()
            self._last_clipboard_text = current_clipboard

        self._update_undo_redo_buttons()

    def _on_text_changed(self):
        """Handle text changes - debounce state saving."""
        self._text_change_timer.start()

    def _on_clipboard_text_changed(self):
        """Handle clipboard text changes - debounce state saving."""
        self._text_change_timer.start()

    def _on_image_delete(self, index: int):
        """Handle image chip delete request."""
        if 0 <= index < len(self._current_images):
            self._save_context_state()
            del self._current_images[index]
            self._rebuild_image_chips()

    def _on_image_copy(self, index: int):
        """Handle image chip copy request."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        if 0 <= index < len(self._image_chips):
            self._image_chips[index].copy_to_clipboard()
            flash_confirmation(self._image_chips[index].copy_btn)

    def _paste_image_from_clipboard(self) -> bool:
        """Paste image from clipboard. Returns True if image was pasted."""
        if not self.clipboard_manager.has_image():
            return False

        image_data = self.clipboard_manager.get_image_data()
        if image_data:
            base64_data, media_type = image_data
            self._save_context_state()
            new_image = ContextItem(
                item_type=ContextItemType.IMAGE,
                data=base64_data,
                media_type=media_type,
            )
            self._current_images.append(new_image)
            self._rebuild_image_chips()

            if self.notification_manager and is_notification_enabled("image_added"):
                self.notification_manager.show_success_notification("Image added")
            return True
        return False

    def _on_save_clicked(self):
        """Save both context and clipboard changes."""
        # Save context
        self.context_manager.clear_context()

        # Add images first
        for image_item in self._current_images:
            self.context_manager.append_context_image(
                image_item.data,
                image_item.media_type or "image/png",
            )

        # Add text if not empty
        text_content = self.text_edit.toPlainText().strip()
        if text_content:
            self.context_manager.append_context(text_content)

        # Save clipboard (only if text edit is visible - image already in clipboard)
        if self.clipboard_edit.isVisible():
            clipboard_content = self.clipboard_edit.toPlainText()
            self.clipboard_manager.set_content(clipboard_content)

        self.context_saved.emit()
        self.accept()

    def keyPressEvent(self, event):
        """Handle key press events."""
        # Ctrl+Z for undo (focus-aware)
        if event.key() == Qt.Key_Z and (event.modifiers() & Qt.ControlModifier):
            if event.modifiers() & Qt.ShiftModifier:
                # Ctrl+Shift+Z for redo
                if self.clipboard_edit.hasFocus():
                    self._redo_clipboard()
                else:
                    self._redo_context()
            else:
                # Ctrl+Z for undo
                if self.clipboard_edit.hasFocus():
                    self._undo_clipboard()
                else:
                    self._undo_context()
            event.accept()
            return

        # Ctrl+Y for redo (alternative, focus-aware)
        if event.key() == Qt.Key_Y and (event.modifiers() & Qt.ControlModifier):
            if self.clipboard_edit.hasFocus():
                self._redo_clipboard()
            else:
                self._redo_context()
            event.accept()
            return

        # Ctrl+C for copy (use xclip to avoid X11 clipboard ownership freeze)
        if (
            event.key() == Qt.Key_C
            and (event.modifiers() & Qt.ControlModifier)
            and not (event.modifiers() & Qt.ShiftModifier)
        ):
            # Check which text edit has focus and get selected text
            focused_edit = None
            if self.text_edit.hasFocus():
                focused_edit = self.text_edit
            elif self.clipboard_edit.hasFocus():
                focused_edit = self.clipboard_edit

            if focused_edit:
                selected_text = focused_edit.textCursor().selectedText()
                if selected_text:
                    # Replace paragraph separators with newlines
                    selected_text = selected_text.replace("\u2029", "\n")
                    self.clipboard_manager.set_content(selected_text)
                    event.accept()
                    return
            # Fall through to default Qt handling if no selection

        # Ctrl+V for paste image (always try if clipboard has image)
        if event.key() == Qt.Key_V and (event.modifiers() & Qt.ControlModifier) and self.clipboard_manager.has_image():
            self._paste_image_from_clipboard()
            event.accept()
            return

        # Escape to close
        if self.handle_escape_key(event):
            return

        super().keyPressEvent(event)

    def eventFilter(self, obj, event):
        """Filter events to intercept Ctrl+Z, Ctrl+C, and Ctrl+V on text edits."""
        if obj in (self.text_edit, self.clipboard_edit) and event.type() == QEvent.KeyPress:
            # Ctrl+Z for undo (focus-aware)
            if event.key() == Qt.Key_Z and (event.modifiers() & Qt.ControlModifier):
                if event.modifiers() & Qt.ShiftModifier:
                    # Ctrl+Shift+Z for redo
                    if obj == self.clipboard_edit:
                        self._redo_clipboard()
                    else:
                        self._redo_context()
                else:
                    # Ctrl+Z for undo
                    if obj == self.clipboard_edit:
                        self._undo_clipboard()
                    else:
                        self._undo_context()
                return True  # Event handled
            # Ctrl+Y for redo (alternative)
            if event.key() == Qt.Key_Y and (event.modifiers() & Qt.ControlModifier):
                if obj == self.clipboard_edit:
                    self._redo_clipboard()
                else:
                    self._redo_context()
                return True  # Event handled
            # Ctrl+C for copy (use xclip to avoid X11 clipboard ownership freeze)
            if (
                event.key() == Qt.Key_C
                and (event.modifiers() & Qt.ControlModifier)
                and not (event.modifiers() & Qt.ShiftModifier)
            ):
                selected_text = obj.textCursor().selectedText()
                if selected_text:
                    # Replace paragraph separators with newlines
                    selected_text = selected_text.replace("\u2029", "\n")
                    self.clipboard_manager.set_content(selected_text)
                    return True  # Event handled
            # Ctrl+V for paste image
            if (
                event.key() == Qt.Key_V
                and (event.modifiers() & Qt.ControlModifier)
                and self.clipboard_manager.has_image()
            ):
                self._paste_image_from_clipboard()
                return True  # Event handled, don't pass to text_edit
        return super().eventFilter(obj, event)
