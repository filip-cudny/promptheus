"""Text preview dialog for displaying and editing content."""

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QFrame, QHBoxLayout, QScrollArea, QVBoxLayout, QWidget

from core.interfaces import ClipboardManager
from modules.gui.shared.base_dialog import BaseDialog
from modules.gui.shared.theme import (
    HEADER_BUTTON_SPACING,
    HEADER_RIGHT_MARGIN,
    SCROLL_CONTENT_MARGINS,
    SCROLL_CONTENT_SPACING,
    SMALL_DIALOG_SIZE,
    SMALL_MIN_DIALOG_SIZE,
    apply_wrap_state,
    create_singleton_dialog_manager,
)
from modules.gui.shared.undo_redo import TextEditUndoHelper
from modules.gui.shared.widgets import create_header_button, create_text_edit

# Singleton dialog manager for this module
_show_dialog = create_singleton_dialog_manager()


def show_preview_dialog(
    title: str,
    content: str,
    clipboard_manager: ClipboardManager | None = None,
):
    """Show a preview dialog with the given title and content. If already open, bring to front."""
    _show_dialog(
        "text_preview",
        lambda: TextPreviewDialog(title, content, clipboard_manager=clipboard_manager),
    )


class TextPreviewDialog(BaseDialog):
    """Dialog for displaying and editing text content with undo/redo support."""

    STATE_KEY = "text_preview_dialog"
    DEFAULT_SIZE = SMALL_DIALOG_SIZE
    MIN_SIZE = SMALL_MIN_DIALOG_SIZE

    def __init__(
        self,
        title: str,
        content: str,
        parent=None,
        clipboard_manager: ClipboardManager | None = None,
    ):
        super().__init__(parent)
        self.setWindowTitle(title)

        self._clipboard_manager = clipboard_manager
        self._wrapped: bool = True  # Default wrapped state
        self._undo_helper: TextEditUndoHelper | None = None

        self._setup_ui(content)
        self.apply_dialog_styles()
        self._restore_state()

    def _setup_ui(self, content: str):
        """Setup the dialog UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 0, 10)  # No right margin for scrollbar
        layout.setSpacing(8)

        # Scroll area for content
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.scroll_area.setFrameShape(QFrame.NoFrame)

        # Container for content
        content_container = QWidget()
        content_layout = QVBoxLayout(content_container)
        content_layout.setContentsMargins(*SCROLL_CONTENT_MARGINS)
        content_layout.setSpacing(SCROLL_CONTENT_SPACING)

        # Toolbar - compact, right-aligned
        toolbar = QHBoxLayout()
        toolbar.setContentsMargins(0, 0, HEADER_RIGHT_MARGIN, 0)
        toolbar.setSpacing(HEADER_BUTTON_SPACING)
        toolbar.addStretch()

        # Wrap toggle button (before undo/redo)
        self.wrap_btn = create_header_button("chevrons-down-up", "Toggle wrap/expand", self._toggle_wrap)
        toolbar.addWidget(self.wrap_btn)

        self.undo_btn = create_header_button("undo", "Undo (Ctrl+Z)", self._undo, enabled=False)
        toolbar.addWidget(self.undo_btn)

        self.redo_btn = create_header_button("redo", "Redo (Ctrl+Shift+Z)", self._redo, enabled=False)
        toolbar.addWidget(self.redo_btn)

        self.copy_btn = create_header_button("copy", "Copy all (Ctrl+Shift+C)", self._copy_all)
        toolbar.addWidget(self.copy_btn)

        content_layout.addLayout(toolbar)

        # Editable text area - stretch=1 makes it fill available space
        self.text_edit = create_text_edit(min_height=100)
        self.text_edit.setPlainText(content or "")
        content_layout.addWidget(self.text_edit, 1)  # stretch=1 to fill space

        # Initialize undo helper after text_edit is created
        self._undo_helper = TextEditUndoHelper(
            self.text_edit,
            self._update_undo_redo_buttons,
            debounce_ms=100,
        )
        self._undo_helper.initialize(content or "")
        self.text_edit.textChanged.connect(self._undo_helper.schedule_save)

        self.scroll_area.setWidget(content_container)
        layout.addWidget(self.scroll_area)

    def _restore_state(self):
        """Restore window geometry and wrap state."""
        self.restore_geometry_from_state()

        # Restore wrap state
        wrapped = self.get_section_state("wrapped", True)
        self._wrapped = wrapped
        apply_wrap_state(self.text_edit, wrapped)
        if not wrapped:
            self.wrap_btn.set_icon("chevrons-up-down")

    def closeEvent(self, event):
        """Save geometry on close."""
        self.save_geometry_to_state()
        super().closeEvent(event)

    def _undo(self):
        """Undo last change."""
        self._undo_helper.undo()

    def _redo(self):
        """Redo last undone change."""
        self._undo_helper.redo()

    def _update_undo_redo_buttons(self, can_undo: bool, can_redo: bool):
        """Update undo/redo button states."""
        self.undo_btn.setEnabled(can_undo)
        self.redo_btn.setEnabled(can_redo)

    def _toggle_wrap(self):
        """Toggle wrap/expand state."""
        self._wrapped = not self._wrapped
        apply_wrap_state(self.text_edit, self._wrapped)
        icon_name = "chevrons-down-up" if self._wrapped else "chevrons-up-down"
        self.wrap_btn.set_icon(icon_name)
        self.save_section_state("wrapped", self._wrapped)

    def _copy_all(self):
        """Copy all text content to clipboard."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        text = self.text_edit.toPlainText()
        if text:
            if self._clipboard_manager:
                self._clipboard_manager.set_content(text)
            else:
                QApplication.clipboard().setText(text)
            flash_confirmation(self.copy_btn)

    def keyPressEvent(self, event):
        """Handle key press events."""
        # Ctrl+Z for undo
        if event.key() == Qt.Key_Z and (event.modifiers() & Qt.ControlModifier):
            if event.modifiers() & Qt.ShiftModifier:
                # Ctrl+Shift+Z for redo
                self._redo()
            else:
                # Ctrl+Z for undo
                self._undo()
            event.accept()
            return

        # Ctrl+Y for redo (alternative)
        if event.key() == Qt.Key_Y and (event.modifiers() & Qt.ControlModifier):
            self._redo()
            event.accept()
            return

        # Ctrl+C for copy (use xclip to avoid X11 clipboard ownership freeze)
        if (
            event.key() == Qt.Key_C
            and (event.modifiers() & Qt.ControlModifier)
            and not (event.modifiers() & Qt.ShiftModifier)
        ) and self._clipboard_manager:
            selected_text = self.text_edit.textCursor().selectedText()
            if selected_text:
                # Replace paragraph separators with newlines
                selected_text = selected_text.replace("\u2029", "\n")
                self._clipboard_manager.set_content(selected_text)
                event.accept()
                return
            # Fall through to default Qt handling if no clipboard_manager or no selection

        # Ctrl+Shift+C for copy all
        if (
            event.key() == Qt.Key_C
            and (event.modifiers() & Qt.ControlModifier)
            and (event.modifiers() & Qt.ShiftModifier)
        ):
            self._copy_all()
            event.accept()
            return

        # Escape to close
        if self.handle_escape_key(event):
            return

        super().keyPressEvent(event)
