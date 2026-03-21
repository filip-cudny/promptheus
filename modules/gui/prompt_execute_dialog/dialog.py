"""Prompt execute dialog for sending custom messages to prompts."""

from collections.abc import Callable

from PySide6.QtCore import QEvent, QSize, Qt, QTimer
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
from core.models import MenuItem
from modules.gui.icons import create_composite_icon, create_icon
from modules.gui.prompt_execute_dialog.conversation_manager import ConversationManager
from modules.gui.prompt_execute_dialog.data import (
    ContextSectionState,
    ConversationNode,
    ConversationTree,
    ConversationTurn,
    OutputState,
    OutputVersionState,
    PromptInputState,
    TabState,
    create_node,
)
from modules.gui.prompt_execute_dialog.message_widgets import (
    AssistantBubble,
    UserMessageBubble,
)
from modules.gui.prompt_execute_dialog.execution_handler import ExecutionHandler
from modules.gui.prompt_execute_dialog.tab_bar import ConversationTabBar
from modules.gui.shared.base_dialog import BaseDialog
from modules.gui.shared.theme import (
    DIALOG_SHOW_DELAY_MS,
    QWIDGETSIZE_MAX,
    SCROLL_CONTENT_MARGINS,
    SCROLL_CONTENT_SPACING,
    TEXT_CHANGE_DEBOUNCE_MS,
    apply_section_size_policy,
    get_text_edit_content_height,
)
from modules.gui.shared.undo_redo import perform_redo, perform_undo
from modules.gui.shared.widgets import (
    TEXT_EDIT_MIN_HEIGHT,
    CollapsibleSectionHeader,
    ImageChipWidget,
    create_text_edit,
)
from modules.utils.notification_config import is_notification_enabled

_open_dialogs: dict[str, "PromptExecuteDialog"] = {}


def _generate_window_key(menu_item: MenuItem) -> str:
    if menu_item.data:
        prompt_id = menu_item.data.get("prompt_id")
        if prompt_id:
            return f"prompt_{prompt_id}"
        prompt_name = menu_item.data.get("prompt_name")
        if prompt_name:
            return f"prompt_name_{prompt_name}"
    return f"menu_item_{menu_item.id}"


def show_prompt_execute_dialog(
    menu_item: MenuItem,
    execution_callback: Callable[[MenuItem, bool], None],
    prompt_store_service=None,
    context_manager: ContextManager | None = None,
    clipboard_manager=None,
    notification_manager=None,
    history_service=None,
    history_entry_id: str | None = None,
):
    """Show the prompt execute dialog.

    Args:
        menu_item: The prompt menu item to execute
        execution_callback: Callback to execute the prompt
        prompt_store_service: The prompt store service for execution
        context_manager: The context manager for loading/saving context
        notification_manager: The notification manager for UI notifications
        history_service: The history service for conversation persistence
        history_entry_id: If provided, restore conversation from this history entry
    """
    window_key = _generate_window_key(menu_item)

    # If restoring from history and dialog already exists, create new tab
    if history_entry_id and window_key in _open_dialogs:
        dialog = _open_dialogs[window_key]
        dialog.raise_()
        dialog.activateWindow()
        # Create new tab and restore from history
        dialog._on_add_tab_clicked()
        dialog.restore_from_history(history_entry_id)
        return

    # Check if dialog for THIS prompt already exists
    if window_key in _open_dialogs:
        dialog = _open_dialogs[window_key]
        dialog.raise_()
        dialog.activateWindow()
        return

    def create_and_show():
        # Double-check after timer delay
        if window_key in _open_dialogs:
            existing = _open_dialogs[window_key]
            existing.raise_()
            existing.activateWindow()
            if history_entry_id:
                existing._on_add_tab_clicked()
                existing.restore_from_history(history_entry_id)
            return

        dialog = PromptExecuteDialog(
            menu_item,
            execution_callback,
            prompt_store_service,
            context_manager,
            clipboard_manager,
            notification_manager,
            history_service=history_service,
        )
        _open_dialogs[window_key] = dialog
        dialog.finished.connect(lambda: _open_dialogs.pop(window_key, None))
        dialog.show()
        dialog.raise_()
        dialog.activateWindow()

        # Restore from history if entry_id provided
        if history_entry_id:
            dialog.restore_from_history(history_entry_id)

    QTimer.singleShot(DIALOG_SHOW_DELAY_MS, create_and_show)


class PromptExecuteDialog(BaseDialog):
    """Dialog for typing a message to send to a prompt."""

    STATE_KEY = "prompt_execute_dialog"

    def __init__(
        self,
        menu_item: MenuItem,
        execution_callback: Callable[[MenuItem, bool], None],
        prompt_store_service=None,
        context_manager: ContextManager | None = None,
        clipboard_manager=None,
        notification_manager=None,
        history_service=None,
        parent=None,
    ):
        super().__init__(parent)
        self.menu_item = menu_item
        self.execution_callback = execution_callback
        self._prompt_store_service = prompt_store_service
        self.context_manager = context_manager
        self.clipboard_manager = clipboard_manager
        self.notification_manager = notification_manager
        self._history_service = history_service
        self._history_entry_id: str | None = None

        # Working state for context section
        self._current_images: list[ContextItem] = []
        self._image_chips: list[ImageChipWidget] = []

        # Working state for message section images
        self._message_images: list[ContextItem] = []
        self._message_image_chips: list[ImageChipWidget] = []

        # Track if output section has been shown
        self._output_section_shown = False

        # Multi-turn conversation state (legacy linear format)
        self._conversation_turns: list[ConversationTurn] = []
        self._current_turn_number: int = 0
        self._dynamic_sections: list[QWidget] = []  # Reply input sections
        self._output_sections: list[QWidget] = []  # Output sections for each turn

        # Tree-based conversation state (new)
        self._conversation_tree: ConversationTree | None = None
        self._message_bubbles: list = []  # UserMessageBubble and AssistantBubble widgets

        # Separate undo/redo stacks for each section
        self._context_undo_stack: list[ContextSectionState] = []
        self._context_redo_stack: list[ContextSectionState] = []
        self._input_undo_stack: list[PromptInputState] = []
        self._input_redo_stack: list[PromptInputState] = []
        self._output_undo_stack: list[OutputState] = []
        self._output_redo_stack: list[OutputState] = []

        # Track last text for debounced state saving
        self._last_context_text = ""
        self._last_input_text = ""
        self._last_output_text = ""

        # Text change debounce timer (created early to avoid timing issues)
        self._text_change_timer = QTimer()
        self._text_change_timer.setSingleShot(True)
        self._text_change_timer.setInterval(TEXT_CHANGE_DEBOUNCE_MS)
        self._text_change_timer.timeout.connect(self._save_text_states)

        # Initialize execution handler
        self._execution_handler = ExecutionHandler(self)

        # Initialize conversation manager
        self._conversation_manager = ConversationManager(self)

        # Tab management
        self._tabs: dict[str, TabState] = {}
        self._active_tab_id: str | None = None
        self._tab_counter: int = 0
        self._tab_bar: ConversationTabBar | None = None
        self._tab_scroll: QScrollArea | None = None

        # Extract prompt name for title
        prompt_name = menu_item.data.get("prompt_name", "Prompt") if menu_item.data else "Prompt"
        self.setWindowTitle(f"Message to: {prompt_name}")

        self._context_supported = True

        self._setup_ui()
        self.apply_dialog_styles()
        self._load_context()

        if not self._prompt_supports_context():
            self._context_supported = False
            self._disable_context_section()

        self._restore_ui_state()

        # Focus message input for immediate typing
        self.input_edit.setFocus()

    # --- Properties for backward compatibility ---

    @property
    def _waiting_for_result(self) -> bool:
        return self._execution_handler.is_waiting

    @property
    def _is_streaming(self) -> bool:
        return self._execution_handler.is_streaming

    @property
    def _current_execution_id(self) -> str | None:
        return self._execution_handler.current_execution_id

    # --- UI Setup ---

    def _setup_ui(self):
        """Set up the dialog UI with sticky context/input and scrollable messages."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 0, 10)  # No right margin - scrollbar sticks to edge
        layout.setSpacing(8)

        # Section 1: Context (STICKY TOP - outside scroll area)
        self.context_section = self._create_context_section()
        layout.addWidget(self.context_section)

        # Scroll area for conversation messages only
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.scroll_area.setFrameShape(QFrame.NoFrame)

        # Container widget for messages
        self.sections_container = QWidget()
        self.sections_layout = QVBoxLayout(self.sections_container)
        self.sections_layout.setContentsMargins(*SCROLL_CONTENT_MARGINS)
        self.sections_layout.setSpacing(SCROLL_CONTENT_SPACING)

        # Add stretch to push messages to top when few
        self.sections_layout.addStretch()

        self.scroll_area.setWidget(self.sections_container)
        layout.addWidget(self.scroll_area, 1)  # stretch factor 1 for scroll area

        # Section 2: Prompt Input (STICKY BOTTOM - outside scroll area)
        self.input_section = self._create_input_section()
        layout.addWidget(self.input_section)

        # Section 3: Output (legacy - kept for backward compatibility)
        # In new tree-based flow, output sections are created as AssistantBubble widgets
        self.output_section = self._create_output_section()
        # Output section is hidden until user clicks Alt+Enter

        # Initialize conversation tree
        self._conversation_tree = ConversationTree()
        self._message_bubbles: list[UserMessageBubble | AssistantBubble] = []

        # Button bar (includes tabs inline)
        self._create_button_bar(layout)

        # Install event filters for keyboard handling
        self.context_text_edit.installEventFilter(self)
        self.input_edit.installEventFilter(self)
        # output_edit event filter is installed when output section is shown

    def _create_context_section(self) -> QWidget:
        """Create the collapsible context section with save button."""
        container = QWidget()
        apply_section_size_policy(container, expanding=False)
        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(4)

        # Header with collapse toggle, wrap button, undo/redo, and save button
        self.context_header = CollapsibleSectionHeader(
            "Context",
            show_save_button=True,
            show_undo_redo=True,
            show_wrap_button=True,
            show_info_button=True,
            hint_text="",
        )
        self.context_header.toggle_requested.connect(self._toggle_context_section)
        self.context_header.wrap_requested.connect(self._toggle_context_wrap)
        self.context_header.save_requested.connect(self._save_context)
        self.context_header.undo_requested.connect(self._undo_context)
        self.context_header.redo_requested.connect(self._redo_context)
        section_layout.addWidget(self.context_header)

        # Images row
        self.context_images_container = QWidget()
        self.context_images_container.setStyleSheet("background: transparent;")
        self.context_images_layout = QHBoxLayout(self.context_images_container)
        self.context_images_layout.setContentsMargins(0, 0, 0, 4)
        self.context_images_layout.setSpacing(6)
        self.context_images_layout.addStretch()
        section_layout.addWidget(self.context_images_container)
        self.context_images_container.hide()  # Hidden if no images

        # Text edit area - context uses smaller min height since it has max constraint
        self.context_text_edit = create_text_edit(
            placeholder="Context content...",
            min_height=100,
        )
        self.context_text_edit.setMaximumHeight(TEXT_EDIT_MIN_HEIGHT)  # Default wrapped height
        self.context_text_edit.textChanged.connect(self._on_context_text_changed)
        section_layout.addWidget(self.context_text_edit)

        return container

    def _create_input_section(self) -> QWidget:
        """Create the collapsible prompt input section (no save button)."""
        container = QWidget()
        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(4)

        # Header with collapse toggle, wrap button, and undo/redo (NO save button)
        self.input_header = CollapsibleSectionHeader(
            "Message",
            show_save_button=False,
            show_undo_redo=True,
            show_wrap_button=True,
            hint_text="",
        )
        self.input_header.toggle_requested.connect(self._toggle_input_section)
        self.input_header.wrap_requested.connect(self._toggle_input_wrap)
        self.input_header.undo_requested.connect(self._undo_input)
        self.input_header.redo_requested.connect(self._redo_input)
        section_layout.addWidget(self.input_header)

        # Images row (for pasted images)
        self.message_images_container = QWidget()
        self.message_images_container.setStyleSheet("background: transparent;")
        self.message_images_layout = QHBoxLayout(self.message_images_container)
        self.message_images_layout.setContentsMargins(0, 0, 0, 4)
        self.message_images_layout.setSpacing(6)
        self.message_images_layout.addStretch()
        section_layout.addWidget(self.message_images_container)
        self.message_images_container.hide()  # Hidden if no images

        # Text edit area
        self.input_edit = create_text_edit(
            placeholder="Type your message...\n(Ctrl+Enter: Close & get result to clipboard | Enter: Send & show | Ctrl+V: Paste image)"
        )
        self.input_edit.setToolTip("Type and send message with prompt")
        self.input_edit.textChanged.connect(self._on_input_text_changed)
        section_layout.addWidget(self.input_edit)

        apply_section_size_policy(container, expanding=True, widget=self.input_edit)

        return container

    def _create_output_section(self) -> QWidget:
        """Create the collapsible output section (no save button)."""
        container = QWidget()
        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(4)

        # Header with collapse toggle, wrap button, and undo/redo (NO save button)
        self.output_header = CollapsibleSectionHeader(
            "Output #1",
            show_save_button=False,
            show_undo_redo=True,
            show_wrap_button=True,
            show_version_nav=True,
            hint_text="",
        )
        self.output_header.toggle_requested.connect(self._toggle_output_section)
        self.output_header.wrap_requested.connect(self._toggle_output_wrap)
        self.output_header.undo_requested.connect(self._undo_output)
        self.output_header.redo_requested.connect(self._redo_output)
        self.output_header.version_prev_requested.connect(self._on_version_prev_output1)
        self.output_header.version_next_requested.connect(self._on_version_next_output1)
        section_layout.addWidget(self.output_header)

        # Text edit area
        self.output_edit = create_text_edit(placeholder="Output will appear here...")
        self.output_edit.textChanged.connect(self._on_output_text_changed)
        section_layout.addWidget(self.output_edit)

        apply_section_size_policy(container, expanding=True, widget=self.output_edit)

        return container

    def _create_button_bar(self, layout: QVBoxLayout):
        """Create button bar with tabs inline and send actions."""
        button_widget = QWidget()
        button_widget.setFixedHeight(44)
        button_bar = QHBoxLayout(button_widget)
        button_bar.setContentsMargins(12, 0, 12, 0)

        # Add tab button on the left (plus icon)
        self.add_tab_btn = QPushButton()
        self.add_tab_btn.setIcon(create_icon("plus", "#888888", 16))
        self.add_tab_btn.setToolTip("New conversation tab")
        self.add_tab_btn.clicked.connect(self._on_add_tab_clicked)
        button_bar.addWidget(self.add_tab_btn)

        # Tab bar in horizontal scroll area
        self._tab_scroll = QScrollArea()
        self._tab_scroll.setWidgetResizable(True)
        self._tab_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self._tab_scroll.setVerticalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self._tab_scroll.setFrameShape(QFrame.NoFrame)
        self._tab_scroll.setStyleSheet("""
            QScrollArea {
                background: transparent;
                border: none;
            }
            QScrollArea > QWidget > QWidget {
                background: transparent;
            }
            QScrollBar:horizontal {
                height: 6px;
                background: transparent;
                margin: 0;
            }
            QScrollBar::handle:horizontal {
                background: #555555;
                border-radius: 3px;
                min-width: 20px;
            }
            QScrollBar::handle:horizontal:hover {
                background: #666666;
            }
            QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {
                width: 0;
                height: 0;
            }
            QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal {
                background: transparent;
            }
        """)
        self._tab_scroll.setFixedHeight(40)

        self._tab_bar = ConversationTabBar()
        self._tab_bar.tab_selected.connect(self._on_tab_selected)
        self._tab_bar.tab_close_requested.connect(self._on_tab_close_requested)
        self._tab_scroll.setWidget(self._tab_bar)
        self._tab_scroll.hide()
        button_bar.addWidget(self._tab_scroll, 1)  # stretch factor 1

        button_bar.addStretch()

        # Send & Show button (Enter)
        self.send_show_btn = QPushButton()
        self.send_show_btn.setIcon(create_icon("send-horizontal", "#444444", 16))
        self.send_show_btn.setToolTip("Send & Show Result (Enter)")
        self.send_show_btn.clicked.connect(self._on_send_show)
        self.send_show_btn.setEnabled(False)  # Disabled until message has content
        button_bar.addWidget(self.send_show_btn)

        # Send & Copy button (Ctrl+Enter) - default
        self.send_copy_btn = QPushButton()
        self.send_copy_btn.setIcon(create_composite_icon("delete", "copy", "#444444", 16, "&", 4))
        self.send_copy_btn.setIconSize(QSize(48, 16))
        self.send_copy_btn.setToolTip("Execute, close, get result to clipboard (Ctrl+Enter)")
        self.send_copy_btn.clicked.connect(self._on_send_copy)
        self.send_copy_btn.setDefault(True)
        self.send_copy_btn.setEnabled(False)  # Disabled until message has content
        button_bar.addWidget(self.send_copy_btn)

        layout.addWidget(button_widget)

    # --- Context loading/saving ---

    def _load_context(self):
        """Load context from context_manager."""
        if not self.context_manager:
            return

        items = self.context_manager.get_items()

        # Separate images and text
        self._current_images = [
            ContextItem(item_type=item.item_type, data=item.data, media_type=item.media_type)
            for item in items
            if item.item_type == ContextItemType.IMAGE
        ]

        text_items = [item.content for item in items if item.item_type == ContextItemType.TEXT and item.content]
        text_content = "\n".join(text_items)

        self._rebuild_image_chips()
        self.context_text_edit.setPlainText(text_content)
        self._last_context_text = text_content

        # Clear undo/redo stacks
        self._context_undo_stack.clear()
        self._context_redo_stack.clear()
        self._update_undo_redo_buttons()

    def _prompt_supports_context(self) -> bool:
        """Check if the current prompt uses the {{context}} placeholder."""
        try:
            if not self._prompt_store_service or not self.menu_item.data:
                return True
            prompt_id = self.menu_item.data.get("prompt_id")
            if not prompt_id:
                return True
            provider = self._prompt_store_service.primary_provider
            if not provider:
                return True
            messages = provider.get_prompt_messages(prompt_id)
            if not messages:
                return True
            return any("{{context}}" in msg.get("content", "") for msg in messages)
        except Exception:
            return True

    def _disable_context_section(self):
        """Disable context section for prompts that don't use {{context}}."""
        self.context_header.set_info_tooltip(
            "Context is only available for prompts that use the {{context}} placeholder"
        )

        self.context_header.set_all_buttons_enabled(False)

        self.context_text_edit.setReadOnly(True)
        self.context_text_edit.setStyleSheet("QTextEdit { color: #666666; }")

    def _save_context(self):
        """Save context changes to context_manager."""
        if not self._context_supported or not self.context_manager:
            return

        self.context_manager.clear_context()

        # Add images first
        for image_item in self._current_images:
            self.context_manager.append_context_image(image_item.data, image_item.media_type or "image/png")

        # Add text
        text_content = self.context_text_edit.toPlainText().strip()
        if text_content:
            self.context_manager.append_context(text_content)

        # Show success notification
        if self.notification_manager and is_notification_enabled("context_saved"):
            self.notification_manager.show_success_notification("Context Saved")

    def _rebuild_image_chips(self):
        """Rebuild image chips from current state."""
        for chip in self._image_chips:
            chip.deleteLater()
        self._image_chips.clear()

        while self.context_images_layout.count():
            self.context_images_layout.takeAt(0)

        if not self._current_images:
            self.context_images_container.hide()
            self._update_context_header_highlight()
            return

        self.context_images_container.show()

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
            self.context_images_layout.addWidget(chip)

        self.context_images_layout.addStretch()
        self._update_context_header_highlight()

    def _on_image_delete(self, index: int):
        """Handle image chip delete request."""
        if 0 <= index < len(self._current_images):
            self._save_context_state()
            del self._current_images[index]
            self._rebuild_image_chips()

    def _on_image_copy(self, index: int):
        """Handle image chip copy request."""
        if 0 <= index < len(self._image_chips):
            self._image_chips[index].copy_to_clipboard()

    def _paste_image_from_clipboard(self) -> bool:
        """Paste image from clipboard to context. Returns True if image was pasted."""
        if not self.clipboard_manager or not self.clipboard_manager.has_image():
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
            return True
        return False

    # --- Message image methods ---

    def _rebuild_message_image_chips(self):
        """Rebuild message image chips from current state."""
        for chip in self._message_image_chips:
            chip.deleteLater()
        self._message_image_chips.clear()

        while self.message_images_layout.count():
            self.message_images_layout.takeAt(0)

        if not self._message_images:
            self.message_images_container.hide()
            return

        self.message_images_container.show()

        for idx, item in enumerate(self._message_images):
            chip = ImageChipWidget(
                index=idx,
                image_number=idx + 1,
                image_data=item.data or "",
                media_type=item.media_type or "image/png",
            )
            chip.delete_requested.connect(self._on_message_image_delete)
            chip.copy_requested.connect(self._on_message_image_copy)
            self._message_image_chips.append(chip)
            self.message_images_layout.addWidget(chip)

        self.message_images_layout.addStretch()

    def _on_message_image_delete(self, index: int):
        """Handle message image chip delete request."""
        if 0 <= index < len(self._message_images):
            del self._message_images[index]
            self._rebuild_message_image_chips()

    def _on_message_image_copy(self, index: int):
        """Handle message image chip copy request."""
        if 0 <= index < len(self._message_image_chips):
            self._message_image_chips[index].copy_to_clipboard()

    def _paste_image_to_message(self) -> bool:
        """Paste image from clipboard to message. Returns True if image was pasted."""
        if not self.clipboard_manager or not self.clipboard_manager.has_image():
            return False

        image_data = self.clipboard_manager.get_image_data()
        if image_data:
            base64_data, media_type = image_data
            new_image = ContextItem(
                item_type=ContextItemType.IMAGE,
                data=base64_data,
                media_type=media_type,
            )
            self._message_images.append(new_image)
            self._rebuild_message_image_chips()
            self._update_send_buttons_state()  # Message now has content (image)
            return True
        return False

    # --- Section toggle methods ---

    def _toggle_context_section(self):
        """Toggle context section visibility."""
        is_visible = self.context_text_edit.isVisible()
        self.context_text_edit.setVisible(not is_visible)
        self.context_images_container.setVisible(not is_visible and len(self._current_images) > 0)
        self.context_header.set_collapsed(is_visible)
        if is_visible:
            self.context_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        else:
            self.context_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Maximum)
        self.save_section_state("context_collapsed", is_visible)
        self._update_context_header_highlight()

    def _update_context_header_highlight(self):
        """Update context header highlight based on content presence."""
        has_content = bool(self._current_images) or bool(self.context_text_edit.toPlainText().strip())
        self.context_header.set_has_content(has_content)

    def _toggle_input_section(self):
        """Toggle input section visibility."""
        self.toggle_section_collapsed(
            "input",
            self.input_header,
            self.input_edit,
            self.input_section,
            expanding=True,
        )

    def _toggle_output_section(self):
        """Toggle output section visibility."""
        if not self._output_section_shown:
            self._expand_output_section()
            return

        self.toggle_section_collapsed(
            "output",
            self.output_header,
            self.output_edit,
            self.output_section,
            expanding=True,
        )

    def _toggle_context_wrap(self):
        """Toggle context section wrap state."""
        is_wrapped = self.context_header.is_wrapped()
        new_wrapped = not is_wrapped
        self.context_header.set_wrap_state(new_wrapped)
        if new_wrapped:
            self.context_text_edit.setMinimumHeight(100)
            self.context_text_edit.setMaximumHeight(TEXT_EDIT_MIN_HEIGHT)
        else:
            content_height = get_text_edit_content_height(self.context_text_edit)
            self.context_text_edit.setMinimumHeight(content_height)
            self.context_text_edit.setMaximumHeight(QWIDGETSIZE_MAX)
        self.save_section_state("context_wrapped", new_wrapped)

    def _toggle_input_wrap(self):
        """Toggle input section wrap state."""
        is_wrapped = self.input_header.is_wrapped()
        new_wrapped = not is_wrapped
        self.input_header.set_wrap_state(new_wrapped)
        if new_wrapped:
            self.input_edit.setMinimumHeight(TEXT_EDIT_MIN_HEIGHT)
        else:
            content_height = get_text_edit_content_height(self.input_edit)
            self.input_edit.setMinimumHeight(content_height)
        self.save_section_state("input_wrapped", new_wrapped)

    def _toggle_output_wrap(self):
        """Toggle output section wrap state."""
        is_wrapped = self.output_header.is_wrapped()
        new_wrapped = not is_wrapped
        self.output_header.set_wrap_state(new_wrapped)
        if new_wrapped:
            self.output_edit.setMinimumHeight(TEXT_EDIT_MIN_HEIGHT)
        else:
            content_height = get_text_edit_content_height(self.output_edit)
            self.output_edit.setMinimumHeight(content_height)
        self.save_section_state("output_wrapped", new_wrapped)

    def _scroll_to_bottom(self):
        """Scroll the scroll area to the bottom."""
        # Use a small delay to ensure layout is complete before scrolling
        QTimer.singleShot(50, self._do_scroll_to_bottom)

    def _do_scroll_to_bottom(self):
        """Perform the actual scroll to bottom."""
        self.scroll_area.verticalScrollBar().setValue(self.scroll_area.verticalScrollBar().maximum())

    # --- UI state persistence ---

    def _restore_ui_state(self):
        """Restore collapsed and wrap states from saved state."""
        self.restore_geometry_from_state()

        self.restore_section_collapsed(
            "context",
            self.context_header,
            [self.context_text_edit, self.context_images_container],
            self.context_section,
        )
        self.restore_section_collapsed("input", self.input_header, self.input_edit, self.input_section)

        context_wrapped = self.get_section_state("context_wrapped", True)
        input_wrapped = self.get_section_state("input_wrapped", True)
        output_wrapped = self.get_section_state("output_wrapped", True)

        self.context_header.set_wrap_state(context_wrapped)
        if not context_wrapped:
            content_height = get_text_edit_content_height(self.context_text_edit)
            self.context_text_edit.setMinimumHeight(content_height)
            self.context_text_edit.setMaximumHeight(QWIDGETSIZE_MAX)

        self.input_header.set_wrap_state(input_wrapped)
        if not input_wrapped:
            content_height = get_text_edit_content_height(self.input_edit)
            self.input_edit.setMinimumHeight(content_height)

        self.output_header.set_wrap_state(output_wrapped)
        if not output_wrapped:
            content_height = get_text_edit_content_height(self.output_edit)
            self.output_edit.setMinimumHeight(content_height)

    def closeEvent(self, event):
        """Save geometry on close and disconnect signals."""
        # Clean up all tab handlers
        for state in self._tabs.values():
            if state.execution_handler:
                state.execution_handler.cleanup()

        # Clean up current execution handler (may not be in tabs dict if no tabs created)
        self._execution_handler.cleanup()

        # BaseDialog handles geometry save
        super().closeEvent(event)

    # --- Undo/Redo: Context Section ---

    def _get_context_state(self) -> ContextSectionState:
        """Get current context state."""
        return ContextSectionState(
            images=[
                ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
                for img in self._current_images
            ],
            text=self.context_text_edit.toPlainText(),
        )

    def _restore_context_state(self, state: ContextSectionState):
        """Restore context state."""
        self._current_images = [
            ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type) for img in state.images
        ]
        self._rebuild_image_chips()
        self.context_text_edit.blockSignals(True)
        self.context_text_edit.setPlainText(state.text)
        self._last_context_text = state.text
        self.context_text_edit.blockSignals(False)

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

    # --- Undo/Redo: Input Section ---

    def _get_input_state(self) -> PromptInputState:
        """Get current input state."""
        return PromptInputState(text=self.input_edit.toPlainText())

    def _restore_input_state(self, state: PromptInputState):
        """Restore input state."""
        self.input_edit.blockSignals(True)
        self.input_edit.setPlainText(state.text)
        self._last_input_text = state.text
        self.input_edit.blockSignals(False)

    def _undo_input(self):
        """Undo last input change."""
        if perform_undo(
            self._input_undo_stack,
            self._input_redo_stack,
            self._get_input_state,
            self._restore_input_state,
        ):
            self._update_undo_redo_buttons()

    def _redo_input(self):
        """Redo last undone input change."""
        if perform_redo(
            self._input_undo_stack,
            self._input_redo_stack,
            self._get_input_state,
            self._restore_input_state,
        ):
            self._update_undo_redo_buttons()

    # --- Undo/Redo: Output Section ---

    def _get_output_state(self) -> OutputState:
        """Get current output state."""
        return OutputState(text=self.output_edit.toPlainText())

    def _restore_output_state(self, state: OutputState):
        """Restore output state."""
        self.output_edit.blockSignals(True)
        self.output_edit.setPlainText(state.text)
        self._last_output_text = state.text
        self.output_edit.blockSignals(False)

    def _undo_output(self):
        """Undo last output change."""
        state = perform_undo(
            self._output_undo_stack,
            self._output_redo_stack,
            self._get_output_state,
            self._restore_output_state,
        )
        if state:
            if self._conversation_turns:
                turn = self._conversation_turns[0]
                if turn.output_versions:
                    turn.output_versions[turn.current_version_index] = state.text
            self._update_undo_redo_buttons()

    def _redo_output(self):
        """Redo last undone output change."""
        state = perform_redo(
            self._output_undo_stack,
            self._output_redo_stack,
            self._get_output_state,
            self._restore_output_state,
        )
        if state:
            if self._conversation_turns:
                turn = self._conversation_turns[0]
                if turn.output_versions:
                    turn.output_versions[turn.current_version_index] = state.text
            self._update_undo_redo_buttons()

    # --- Version Navigation for Output #1 ---

    def _save_current_version_undo_state(self, turn: ConversationTurn):
        """Save current undo/redo state for the active version."""
        if not turn.output_versions:
            return
        idx = turn.current_version_index
        while len(turn.version_undo_states) <= idx:
            turn.version_undo_states.append(OutputVersionState())
        turn.version_undo_states[idx].undo_stack = list(self._output_undo_stack)
        turn.version_undo_states[idx].redo_stack = list(self._output_redo_stack)
        turn.version_undo_states[idx].last_text = self._last_output_text

    def _restore_version_undo_state(self, turn: ConversationTurn):
        """Restore undo/redo state for the active version."""
        idx = turn.current_version_index
        if idx < len(turn.version_undo_states):
            state = turn.version_undo_states[idx]
            self._output_undo_stack = list(state.undo_stack)
            self._output_redo_stack = list(state.redo_stack)
            self._last_output_text = state.last_text
        else:
            self._output_undo_stack.clear()
            self._output_redo_stack.clear()
            self._last_output_text = turn.output_versions[idx] if turn.output_versions else ""
        self._update_undo_redo_buttons()

    def _on_version_prev_output1(self):
        """Navigate to previous version for Output #1."""
        if not self._conversation_turns:
            return
        turn = self._conversation_turns[0]
        if turn.current_version_index <= 0:
            return
        current_text = self.output_edit.toPlainText()
        turn.output_versions[turn.current_version_index] = current_text
        self._save_current_version_undo_state(turn)
        turn.current_version_index -= 1
        self._apply_version_to_output1(turn)

    def _on_version_next_output1(self):
        """Navigate to next version for Output #1."""
        if not self._conversation_turns:
            return
        turn = self._conversation_turns[0]
        if turn.current_version_index >= len(turn.output_versions) - 1:
            return
        current_text = self.output_edit.toPlainText()
        turn.output_versions[turn.current_version_index] = current_text
        self._save_current_version_undo_state(turn)
        turn.current_version_index += 1
        self._apply_version_to_output1(turn)

    def _apply_version_to_output1(self, turn: ConversationTurn):
        """Apply the current version to Output #1."""
        text = turn.output_versions[turn.current_version_index]
        turn.output_text = text
        self.output_edit.blockSignals(True)
        self.output_edit.setPlainText(text)
        self.output_edit.blockSignals(False)
        self.output_header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))
        self._restore_version_undo_state(turn)

    # --- Version Navigation for Dynamic Output Sections ---

    def _save_dynamic_version_undo_state(self, section: QWidget, turn: ConversationTurn):
        """Save current undo/redo state for the active version in dynamic section."""
        if not turn.output_versions:
            return
        idx = turn.current_version_index
        while len(turn.version_undo_states) <= idx:
            turn.version_undo_states.append(OutputVersionState())
        turn.version_undo_states[idx].undo_stack = list(section.undo_stack)
        turn.version_undo_states[idx].redo_stack = list(section.redo_stack)
        turn.version_undo_states[idx].last_text = section.last_text

    def _restore_dynamic_version_undo_state(self, section: QWidget, turn: ConversationTurn):
        """Restore undo/redo state for the active version in dynamic section."""
        idx = turn.current_version_index
        if idx < len(turn.version_undo_states):
            state = turn.version_undo_states[idx]
            section.undo_stack = list(state.undo_stack)
            section.redo_stack = list(state.redo_stack)
            section.last_text = state.last_text
        else:
            section.undo_stack.clear()
            section.redo_stack.clear()
            section.last_text = turn.output_versions[idx] if turn.output_versions else ""
        self._update_dynamic_section_buttons(section)

    def _find_turn_for_output_section(self, section: QWidget) -> ConversationTurn:
        """Find the ConversationTurn that corresponds to a dynamic output section."""
        turn_number = getattr(section, "turn_number", None)
        if turn_number is None:
            return None
        for turn in self._conversation_turns:
            if turn.turn_number == turn_number:
                return turn
        return None

    def _on_version_prev_dynamic(self, section: QWidget):
        """Navigate to previous version for a dynamic output section."""
        turn = self._find_turn_for_output_section(section)
        if not turn or turn.current_version_index <= 0:
            return
        current_text = section.text_edit.toPlainText()
        turn.output_versions[turn.current_version_index] = current_text
        self._save_dynamic_version_undo_state(section, turn)
        turn.current_version_index -= 1
        self._apply_version_to_dynamic(section, turn)

    def _on_version_next_dynamic(self, section: QWidget):
        """Navigate to next version for a dynamic output section."""
        turn = self._find_turn_for_output_section(section)
        if not turn or turn.current_version_index >= len(turn.output_versions) - 1:
            return
        current_text = section.text_edit.toPlainText()
        turn.output_versions[turn.current_version_index] = current_text
        self._save_dynamic_version_undo_state(section, turn)
        turn.current_version_index += 1
        self._apply_version_to_dynamic(section, turn)

    def _apply_version_to_dynamic(self, section: QWidget, turn: ConversationTurn):
        """Apply the current version to a dynamic output section."""
        text = turn.output_versions[turn.current_version_index]
        turn.output_text = text
        section.text_edit.blockSignals(True)
        section.text_edit.setPlainText(text)
        section.text_edit.blockSignals(False)
        section.header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))
        self._restore_dynamic_version_undo_state(section, turn)

    # --- Common undo/redo ---

    def _update_undo_redo_buttons(self):
        """Update undo/redo button states for all sections."""
        self.context_header.set_undo_redo_enabled(
            len(self._context_undo_stack) > 0,
            len(self._context_redo_stack) > 0,
        )
        self.input_header.set_undo_redo_enabled(
            len(self._input_undo_stack) > 0,
            len(self._input_redo_stack) > 0,
        )
        self.output_header.set_undo_redo_enabled(
            len(self._output_undo_stack) > 0,
            len(self._output_redo_stack) > 0,
        )

    # --- Dynamic section undo/redo ---

    def _undo_dynamic_section(self, section: QWidget):
        """Undo last change in a dynamic section."""
        if not section.undo_stack:
            return
        current = section.text_edit.toPlainText()
        section.redo_stack.append(current)
        previous = section.undo_stack.pop()
        section.text_edit.blockSignals(True)
        section.text_edit.setPlainText(previous)
        section.text_edit.blockSignals(False)
        section.last_text = previous

        turn = self._find_turn_for_output_section(section)
        if turn and turn.output_versions:
            turn.output_versions[turn.current_version_index] = previous

        self._update_dynamic_section_buttons(section)

    def _redo_dynamic_section(self, section: QWidget):
        """Redo last undone change in a dynamic section."""
        if not section.redo_stack:
            return
        current = section.text_edit.toPlainText()
        section.undo_stack.append(current)
        next_state = section.redo_stack.pop()
        section.text_edit.blockSignals(True)
        section.text_edit.setPlainText(next_state)
        section.text_edit.blockSignals(False)
        section.last_text = next_state

        turn = self._find_turn_for_output_section(section)
        if turn and turn.output_versions:
            turn.output_versions[turn.current_version_index] = next_state

        self._update_dynamic_section_buttons(section)

    def _schedule_dynamic_state_save(self, section: QWidget):
        """Schedule state save for dynamic section (debounced)."""
        if not hasattr(section, "_save_timer"):
            section._save_timer = QTimer()
            section._save_timer.setSingleShot(True)
            section._save_timer.setInterval(TEXT_CHANGE_DEBOUNCE_MS)
            section._save_timer.timeout.connect(lambda s=section: self._save_dynamic_state(s))
        section._save_timer.start()

    def _save_dynamic_state(self, section: QWidget):
        """Save state for a dynamic section if text changed."""
        current = section.text_edit.toPlainText()
        if current != section.last_text:
            section.undo_stack.append(section.last_text)
            section.redo_stack.clear()
            section.last_text = current
            self._update_dynamic_section_buttons(section)
            turn = self._find_turn_for_output_section(section)
            if turn and turn.output_versions:
                turn.output_versions[turn.current_version_index] = current

    def _update_dynamic_section_buttons(self, section: QWidget):
        """Update undo/redo buttons for a dynamic section."""
        section.header.set_undo_redo_enabled(len(section.undo_stack) > 0, len(section.redo_stack) > 0)

    def _update_dynamic_section_height(self, section: QWidget):
        """Update height for dynamic section when unwrapped."""
        if not section.header.is_wrapped():
            content_height = get_text_edit_content_height(section.text_edit)
            section.text_edit.setMinimumHeight(content_height)

    # --- Section deletion (delegated to ConversationManager) ---

    def _delete_section(self, section: QWidget):
        """Delete a single dynamic section (only the one clicked)."""
        self._conversation_manager.delete_section(section)

    def _update_delete_button_visibility(self):
        """Show delete button only on the absolute last section (bottom-most)."""
        self._conversation_manager.update_delete_button_visibility()

    def _renumber_sections(self):
        """Update section headers to reflect correct visual numbering."""
        self._conversation_manager.renumber_sections()

    def _has_empty_conversation_sections(self) -> bool:
        """Check if there are empty sections in conversation history (excluding current input)."""
        return self._conversation_manager.has_empty_conversation_sections()

    def _update_send_buttons_state(self):
        """Enable/disable send buttons based on content AND global execution state."""
        # Check current input section (could be original or reply)
        if self._dynamic_sections:
            section = self._dynamic_sections[-1]
            has_text = bool(section.text_edit.toPlainText().strip())
            has_images = bool(section.turn_images)
        else:
            has_text = bool(self.input_edit.toPlainText().strip())
            has_images = bool(self._message_images)

        has_message = has_text or has_images

        # Check if in regenerate mode
        is_regenerate = self._is_regenerate_mode()

        # Check for empty sections in conversation history
        has_conversation_error = self._has_empty_conversation_sections()

        # Check if stop button is active - don't override its state
        stop_active = self._execution_handler._stop_button_active

        can_send = (
            has_message
            and not has_conversation_error
            and not self._waiting_for_result
        )

        can_act = (
            (has_message or is_regenerate)
            and not has_conversation_error
            and not self._waiting_for_result
        )

        # Enable buttons (stop button should stay enabled)
        self.send_show_btn.setEnabled(can_act or stop_active == "alt")
        self.send_copy_btn.setEnabled(can_send or stop_active == "ctrl")

        # Only update send_show_btn icon if not in stop mode
        if stop_active != "alt":
            if is_regenerate:
                icon_color = "#f0f0f0" if can_act else "#444444"
                self.send_show_btn.setIcon(create_icon("refresh-cw", icon_color, 16))
                self.send_show_btn.setToolTip("Regenerate (Enter)")
            else:
                icon_color = "#f0f0f0" if can_act else "#444444"
                self.send_show_btn.setIcon(create_icon("send-horizontal", icon_color, 16))
                self.send_show_btn.setToolTip("Send & Show Result (Enter)")

        # Only update send_copy_btn icon if not in stop mode
        if stop_active != "ctrl":
            copy_icon_color = "#f0f0f0" if can_send else "#444444"
            self.send_copy_btn.setIcon(create_composite_icon("delete", "copy", copy_icon_color, 16, "&", 4))

    def _on_context_text_changed(self):
        """Handle context text changes - debounce state saving."""
        if not self._context_supported:
            return
        self._text_change_timer.start()
        if not self.context_header.is_wrapped():
            content_height = get_text_edit_content_height(self.context_text_edit)
            self.context_text_edit.setMinimumHeight(content_height)
        self._update_context_header_highlight()

    def _on_input_text_changed(self):
        """Handle input text changes - debounce state saving and update buttons."""
        self._text_change_timer.start()
        self._update_send_buttons_state()
        if not self.input_header.is_wrapped():
            content_height = get_text_edit_content_height(self.input_edit)
            self.input_edit.setMinimumHeight(content_height)

    def _on_output_text_changed(self):
        """Handle output text changes - debounce state saving."""
        self._text_change_timer.start()
        if not self.output_header.is_wrapped():
            content_height = get_text_edit_content_height(self.output_edit)
            self.output_edit.setMinimumHeight(content_height)

    def _save_text_states(self):
        """Save state if text has significantly changed in any section."""
        # Context
        current_context = self.context_text_edit.toPlainText()
        if current_context != self._last_context_text:
            state = ContextSectionState(
                images=[
                    ContextItem(
                        item_type=img.item_type,
                        data=img.data,
                        media_type=img.media_type,
                    )
                    for img in self._current_images
                ],
                text=self._last_context_text,
            )
            self._context_undo_stack.append(state)
            self._context_redo_stack.clear()
            self._last_context_text = current_context

        # Input
        current_input = self.input_edit.toPlainText()
        if current_input != self._last_input_text:
            state = PromptInputState(text=self._last_input_text)
            self._input_undo_stack.append(state)
            self._input_redo_stack.clear()
            self._last_input_text = current_input

        # Output
        current_output = self.output_edit.toPlainText()
        if current_output != self._last_output_text:
            state = OutputState(text=self._last_output_text)
            self._output_undo_stack.append(state)
            self._output_redo_stack.clear()
            self._last_output_text = current_output
            if self._conversation_turns:
                turn = self._conversation_turns[0]
                if turn.output_versions:
                    turn.output_versions[turn.current_version_index] = current_output

        self._update_undo_redo_buttons()

    def _sync_all_outputs_to_versions(self):
        """Sync all output text edits to turn.output_versions arrays."""
        if not self._conversation_turns:
            return

        # Sync Output #1 (turn 1)
        turn1 = self._conversation_turns[0]
        if turn1.output_versions:
            turn1.output_versions[turn1.current_version_index] = self.output_edit.toPlainText()

    # --- Conversation Tree Helpers ---

    def _capture_conversation_tree(self) -> ConversationTree | None:
        """Capture the current conversation tree state."""
        if not self._conversation_tree or self._conversation_tree.is_empty():
            return None
        # Sync bubble contents to tree nodes before capture
        self._sync_bubbles_to_tree()
        # Create deep copy
        tree_copy = ConversationTree(
            nodes={
                nid: ConversationNode(
                    node_id=node.node_id,
                    parent_id=node.parent_id,
                    role=node.role,
                    content=node.content,
                    images=[
                        ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
                        for img in node.images
                    ],
                    timestamp=node.timestamp,
                    children=list(node.children),
                    undo_stack=list(node.undo_stack),
                    redo_stack=list(node.redo_stack),
                    last_text=node.last_text,
                )
                for nid, node in self._conversation_tree.nodes.items()
            },
            root_node_id=self._conversation_tree.root_node_id,
            current_path=list(self._conversation_tree.current_path),
        )
        return tree_copy

    def _sync_bubbles_to_tree(self):
        """Sync content from message bubbles back to tree nodes."""
        if not self._conversation_tree:
            return
        for bubble in self._message_bubbles:
            node = self._conversation_tree.get_node(bubble.node_id)
            if node:
                # Skip syncing empty assistant content (preserves streaming content in tree)
                if node.role == "assistant" and not bubble.get_content().strip():
                    continue
                node.content = bubble.get_content()
                if hasattr(bubble, "get_images"):
                    node.images = bubble.get_images()

    def _restore_conversation_tree(self, tree: ConversationTree | None):
        """Restore conversation tree and rebuild message bubbles."""
        self._clear_message_bubbles()
        if not tree:
            self._conversation_tree = ConversationTree()
            return
        self._conversation_tree = tree
        self._rebuild_message_bubbles_from_tree()

    def _clear_message_bubbles(self):
        """Remove all message bubble widgets from layout."""
        for bubble in self._message_bubbles:
            self.sections_layout.removeWidget(bubble)
            bubble.setParent(None)
            bubble.deleteLater()
        self._message_bubbles.clear()

    def _rebuild_message_bubbles_from_tree(self):
        """Rebuild message bubble widgets from the conversation tree."""
        self._clear_message_bubbles()

        if not self._conversation_tree or self._conversation_tree.is_empty():
            return

        pairs = self._conversation_tree.get_message_pairs()
        for i, (user_node, assistant_node) in enumerate(pairs):
            message_number = i + 1

            # Create user message bubble
            user_bubble = UserMessageBubble(
                node_id=user_node.node_id,
                message_number=message_number,
                content=user_node.content,
                images=list(user_node.images),
                show_delete_button=False,
            )
            user_bubble.text_changed.connect(self._on_bubble_text_changed)
            user_bubble.images_changed.connect(self._update_send_buttons_state)
            user_bubble.text_edit.installEventFilter(self)
            self._message_bubbles.append(user_bubble)
            # Insert before stretch
            insert_idx = self.sections_layout.count() - 1
            self.sections_layout.insertWidget(insert_idx, user_bubble)

            # Create assistant bubble if response exists
            if assistant_node:
                assistant_bubble = AssistantBubble(
                    node_id=assistant_node.node_id,
                    output_number=message_number,
                    content=assistant_node.content,
                    show_delete_button=False,
                )
                assistant_bubble.text_changed.connect(self._on_bubble_text_changed)
                assistant_bubble.regenerate_requested.connect(self._on_regenerate_from_bubble)
                assistant_bubble.branch_prev_requested.connect(self._on_branch_prev)
                assistant_bubble.branch_next_requested.connect(self._on_branch_next)
                assistant_bubble.text_edit.installEventFilter(self)
                self._message_bubbles.append(assistant_bubble)
                insert_idx = self.sections_layout.count() - 1
                self.sections_layout.insertWidget(insert_idx, assistant_bubble)

                # Update branch navigation
                siblings, idx = self._conversation_tree.get_siblings(assistant_node.node_id)
                assistant_bubble.set_branch_info(idx + 1, len(siblings))

        self._update_delete_button_visibility()

        # Ensure focus stays on sticky input for next message
        self.input_edit.setFocus()

    def _on_bubble_text_changed(self):
        """Handle text change in any message bubble."""
        self._update_send_buttons_state()

    def _on_regenerate_from_bubble(self, node_id: str):
        """Handle regenerate request from an assistant bubble."""
        if not self._conversation_tree:
            return
        node = self._conversation_tree.get_node(node_id)
        if not node or node.role != "assistant":
            return
        # Regenerate creates a new sibling branch
        self._regenerate_at_node(node_id)

    def _on_branch_prev(self, node_id: str):
        """Navigate to previous branch."""
        if not self._conversation_tree:
            return
        siblings, idx = self._conversation_tree.get_siblings(node_id)
        if idx > 0:
            node = self._conversation_tree.get_node(node_id)
            if node and node.parent_id:
                self._conversation_tree.switch_branch(node.parent_id, idx - 1)
                self._rebuild_message_bubbles_from_tree()

    def _on_branch_next(self, node_id: str):
        """Navigate to next branch."""
        if not self._conversation_tree:
            return
        siblings, idx = self._conversation_tree.get_siblings(node_id)
        if idx < len(siblings) - 1:
            node = self._conversation_tree.get_node(node_id)
            if node and node.parent_id:
                self._conversation_tree.switch_branch(node.parent_id, idx + 1)
                self._rebuild_message_bubbles_from_tree()

    def _regenerate_at_node(self, node_id: str):
        """Regenerate response at a specific assistant node, creating a new branch."""
        if not self._conversation_tree:
            return

        old_node = self._conversation_tree.get_node(node_id)
        if not old_node or old_node.role != "assistant":
            return

        user_node_id = old_node.parent_id
        if not user_node_id:
            return

        user_node = self._conversation_tree.get_node(user_node_id)
        if not user_node:
            return

        self._sync_bubbles_to_tree()

        new_assistant = self._conversation_manager.regenerate_response_in_tree(node_id)
        if not new_assistant:
            return

        self._execution_handler._pending_user_node_id = user_node_id
        self._execution_handler._pending_assistant_node_id = new_assistant.node_id

        self._pending_is_regeneration = True

        self._rebuild_message_bubbles_from_tree()

        self._execution_handler.execute_with_message(
            user_node.content, keep_open=True, regenerate=True
        )

    # --- Tab Management ---

    def _capture_current_state(self) -> TabState:
        """Capture complete state of current conversation for tab switching."""
        # Serialize dynamic sections
        dynamic_sections_data = []
        for section in self._dynamic_sections:
            section_data = {
                "turn_number": section.turn_number,
                "text": section.text_edit.toPlainText(),
                "images": [{"data": img.data, "media_type": img.media_type} for img in section.turn_images],
                "undo_stack": list(section.undo_stack),
                "redo_stack": list(section.redo_stack),
                "last_text": section.last_text,
                "collapsed": section.header.is_collapsed(),
                "wrapped": section.header.is_wrapped(),
            }
            dynamic_sections_data.append(section_data)

        # Serialize output sections
        output_sections_data = []
        for section in self._output_sections:
            section_data = {
                "turn_number": section.turn_number,
                "text": section.text_edit.toPlainText(),
                "undo_stack": list(section.undo_stack),
                "redo_stack": list(section.redo_stack),
                "last_text": section.last_text,
                "collapsed": section.header.is_collapsed(),
                "wrapped": section.header.is_wrapped(),
            }
            output_sections_data.append(section_data)

        return TabState(
            tab_id=self._active_tab_id or "",
            tab_name=f"Tab {self._tab_counter}",
            # Context section
            context_images=[
                ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
                for img in self._current_images
            ],
            context_text=self.context_text_edit.toPlainText(),
            context_undo_stack=list(self._context_undo_stack),
            context_redo_stack=list(self._context_redo_stack),
            last_context_text=self._last_context_text,
            # Message/Input section
            message_images=[
                ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
                for img in self._message_images
            ],
            message_text=self.input_edit.toPlainText(),
            input_undo_stack=list(self._input_undo_stack),
            input_redo_stack=list(self._input_redo_stack),
            last_input_text=self._last_input_text,
            # Output section
            output_text=self.output_edit.toPlainText(),
            output_section_shown=self._output_section_shown,
            output_undo_stack=list(self._output_undo_stack),
            output_redo_stack=list(self._output_redo_stack),
            last_output_text=self._last_output_text,
            # Tree-based conversation (new)
            conversation_tree=self._capture_conversation_tree(),
            # Legacy: Multi-turn conversation
            conversation_turns=list(self._conversation_turns),
            current_turn_number=self._current_turn_number,
            dynamic_sections_data=dynamic_sections_data,
            output_sections_data=output_sections_data,
            # Execution state
            waiting_for_result=self._waiting_for_result,
            is_streaming=self._is_streaming,
            streaming_accumulated=self._execution_handler._streaming_accumulated,
            current_execution_id=self._execution_handler._current_execution_id,
            stop_button_active=self._execution_handler._stop_button_active,
            pending_user_node_id=self._execution_handler._pending_user_node_id,
            pending_assistant_node_id=self._execution_handler._pending_assistant_node_id,
            # History tracking
            history_entry_id=self._history_entry_id,
            # UI collapsed/wrapped states
            context_collapsed=self.context_header.is_collapsed(),
            input_collapsed=self.input_header.is_collapsed(),
            output_collapsed=self.output_header.is_collapsed(),
            context_wrapped=self.context_header.is_wrapped(),
            input_wrapped=self.input_header.is_wrapped(),
            output_wrapped=self.output_header.is_wrapped(),
            # Per-tab execution handler
            execution_handler=self._execution_handler,
        )

    def _restore_state(self, state: TabState):
        """Restore complete state from a TabState object."""
        # Clear dynamic sections first
        self._clear_dynamic_sections()

        # Restore context section
        self._current_images = [
            ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
            for img in state.context_images
        ]
        self._rebuild_image_chips()
        self.context_text_edit.blockSignals(True)
        self.context_text_edit.setPlainText(state.context_text)
        self.context_text_edit.blockSignals(False)
        self._context_undo_stack = list(state.context_undo_stack)
        self._context_redo_stack = list(state.context_redo_stack)
        self._last_context_text = state.last_context_text
        self._update_context_header_highlight()

        # Restore message/input section
        self._message_images = [
            ContextItem(item_type=img.item_type, data=img.data, media_type=img.media_type)
            for img in state.message_images
        ]
        self._rebuild_message_image_chips()
        self.input_edit.blockSignals(True)
        self.input_edit.setPlainText(state.message_text)
        self.input_edit.blockSignals(False)
        self._input_undo_stack = list(state.input_undo_stack)
        self._input_redo_stack = list(state.input_redo_stack)
        self._last_input_text = state.last_input_text

        # Restore output section
        self.output_edit.blockSignals(True)
        self.output_edit.setPlainText(state.output_text)
        self.output_edit.blockSignals(False)
        self._output_undo_stack = list(state.output_undo_stack)
        self._output_redo_stack = list(state.output_redo_stack)
        self._last_output_text = state.last_output_text

        # Handle output section visibility
        if state.output_section_shown and not self._output_section_shown:
            self.sections_layout.addWidget(self.output_section)
            self.output_edit.installEventFilter(self)
            self._output_section_shown = True
        elif not state.output_section_shown and self._output_section_shown:
            self.sections_layout.removeWidget(self.output_section)
            self.output_section.setParent(None)
            self._output_section_shown = False

        # Restore multi-turn conversation state (legacy)
        self._conversation_turns = list(state.conversation_turns)
        self._current_turn_number = state.current_turn_number

        # Restore tree-based conversation (new)
        if state.conversation_tree:
            self._restore_conversation_tree(state.conversation_tree)
        else:
            self._conversation_tree = ConversationTree()
            self._clear_message_bubbles()

        # Restore history tracking
        self._history_entry_id = state.history_entry_id

        # Restore dynamic sections (legacy - for backward compatibility)
        self._conversation_manager.restore_dynamic_sections(state.dynamic_sections_data, state.output_sections_data)

        # Restore version display for Output #1
        if self._conversation_turns and self._conversation_turns[0].output_versions:
            turn = self._conversation_turns[0]
            self.output_header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))

        # Restore UI collapsed/wrapped states
        self._restore_section_ui_states(state)

        # Note: Execution handler is switched in _on_tab_selected, not restored here
        # The handler reference is stored in state.execution_handler

        # Update button states
        self._update_undo_redo_buttons()
        self._update_send_buttons_state()
        self._update_delete_button_visibility()

    def _clear_dynamic_sections(self):
        """Remove all dynamic reply and output sections from layout."""
        self._conversation_manager.clear_dynamic_sections()

    def _restore_section_ui_states(self, state: TabState):
        """Restore collapsed and wrapped states for main sections."""
        # Context section
        if state.context_collapsed:
            self.context_text_edit.hide()
            self.context_images_container.hide()
            self.context_header.set_collapsed(True)
            self.context_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        else:
            self.context_text_edit.show()
            if self._current_images:
                self.context_images_container.show()
            self.context_header.set_collapsed(False)
            self.context_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Maximum)

        self.context_header.set_wrap_state(state.context_wrapped)
        if not state.context_wrapped:
            content_height = get_text_edit_content_height(self.context_text_edit)
            self.context_text_edit.setMinimumHeight(content_height)
            self.context_text_edit.setMaximumHeight(QWIDGETSIZE_MAX)
        else:
            self.context_text_edit.setMinimumHeight(100)
            self.context_text_edit.setMaximumHeight(TEXT_EDIT_MIN_HEIGHT)

        # Input section
        if state.input_collapsed:
            self.input_edit.hide()
            self.input_header.set_collapsed(True)
            self.input_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
        else:
            self.input_edit.show()
            self.input_header.set_collapsed(False)
            self.input_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Expanding)

        self.input_header.set_wrap_state(state.input_wrapped)
        if not state.input_wrapped:
            content_height = get_text_edit_content_height(self.input_edit)
            self.input_edit.setMinimumHeight(content_height)

        # Output section
        if self._output_section_shown:
            if state.output_collapsed:
                self.output_edit.hide()
                self.output_header.set_collapsed(True)
                self.output_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Fixed)
            else:
                self.output_edit.show()
                self.output_header.set_collapsed(False)
                self.output_section.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Expanding)

            self.output_header.set_wrap_state(state.output_wrapped)
            if not state.output_wrapped:
                content_height = get_text_edit_content_height(self.output_edit)
                self.output_edit.setMinimumHeight(content_height)

    def _reset_to_initial_state(self):
        """Reset dialog to fresh initial state for a new tab."""
        # Clear dynamic sections
        self._clear_dynamic_sections()

        # Remove output section from layout if shown
        if self._output_section_shown:
            self.sections_layout.removeWidget(self.output_section)
            self.output_section.setParent(None)
            self._output_section_shown = False

        # Reset context - reload from context_manager
        self._context_undo_stack.clear()
        self._context_redo_stack.clear()
        self._load_context()

        # Reset message/input section
        self._message_images.clear()
        self._rebuild_message_image_chips()
        self.input_edit.blockSignals(True)
        self.input_edit.setPlainText("")
        self.input_edit.blockSignals(False)
        self._input_undo_stack.clear()
        self._input_redo_stack.clear()
        self._last_input_text = ""

        # Reset output section
        self.output_edit.blockSignals(True)
        self.output_edit.setPlainText("")
        self.output_edit.blockSignals(False)
        self._output_undo_stack.clear()
        self._output_redo_stack.clear()
        self._last_output_text = ""

        # Reset multi-turn conversation state (legacy)
        self._conversation_turns.clear()
        self._current_turn_number = 0

        # Reset tree-based conversation (new)
        self._conversation_tree = ConversationTree()
        self._clear_message_bubbles()

        # Reset history tracking (new tab = new conversation)
        self._history_entry_id = None

        # Reset version display for Output #1
        self.output_header.set_version_info(0, 0)

        # Reset UI states
        self.context_text_edit.show()
        self.context_header.set_collapsed(False)
        self.input_edit.show()
        self.input_header.set_collapsed(False)

        # Note: When called from _on_add_tab_clicked, handler is already fresh.
        # When called from _on_tab_close_requested (last tab), reset handler state.
        if not self._tabs:
            self._execution_handler._waiting_for_result = False
            self._execution_handler._is_streaming = False
            self._execution_handler._streaming_accumulated = ""
            self._execution_handler._current_execution_id = None
            self._execution_handler._stop_button_active = None
            self._execution_handler._pending_user_node_id = None
            self._execution_handler._pending_assistant_node_id = None

        # Update button states
        self._update_undo_redo_buttons()
        self._update_send_buttons_state()

    def _update_tab_bar_visibility(self):
        """Show tab bar only when there are multiple tabs."""
        has_multiple_tabs = self._tab_bar.get_tab_count() > 1
        self._tab_scroll.setVisible(has_multiple_tabs)
        if has_multiple_tabs:
            self._tab_bar.updateGeometry()
            self._tab_scroll.updateGeometry()

    def _on_add_tab_clicked(self):
        """Handle add tab button click."""
        # If this is the first tab creation, create Tab 1 for current state
        if not self._active_tab_id:
            self._tab_counter += 1
            first_tab_id = f"tab_{self._tab_counter}"
            first_tab_name = f"Tab {self._tab_counter}"

            # Current handler becomes Tab 1's handler
            self._execution_handler._tab_id = first_tab_id
            first_state = self._capture_current_state()
            first_state.tab_id = first_tab_id
            first_state.tab_name = first_tab_name
            first_state.execution_handler = self._execution_handler
            self._tabs[first_tab_id] = first_state
            self._active_tab_id = first_tab_id

            # Add first tab to tab bar
            self._tab_bar.add_tab(first_tab_id, first_tab_name)
        else:
            # Save current tab state (including current handler)
            self._tabs[self._active_tab_id] = self._capture_current_state()

        # Revert button state from old handler before switching
        if self._execution_handler._stop_button_active:
            self._execution_handler._revert_button_to_send_state()

        # Increment tab counter and generate new tab ID
        self._tab_counter += 1
        new_tab_id = f"tab_{self._tab_counter}"
        new_tab_name = f"Tab {self._tab_counter}"

        # Create fresh handler for new tab (no global signal connection - tabs execute independently)
        new_handler = ExecutionHandler(self)
        new_handler._tab_id = new_tab_id
        self._execution_handler = new_handler

        # Reset dialog to initial state
        self._reset_to_initial_state()

        # Create and store new tab state with new handler
        new_state = self._capture_current_state()
        new_state.tab_id = new_tab_id
        new_state.tab_name = new_tab_name
        new_state.execution_handler = new_handler
        self._tabs[new_tab_id] = new_state
        self._active_tab_id = new_tab_id

        # Add tab to tab bar
        self._tab_bar.add_tab(new_tab_id, new_tab_name)
        self._tab_bar.set_active_tab(new_tab_id)
        self._update_tab_bar_visibility()

        # Focus input for immediate typing
        self.input_edit.setFocus()

    def _on_tab_selected(self, tab_id: str):
        """Handle tab selection."""
        if tab_id == self._active_tab_id:
            return

        # Save current tab state (including current handler reference)
        if self._active_tab_id:
            self._tabs[self._active_tab_id] = self._capture_current_state()

        # Revert button state from old handler (if it had stop button active)
        if self._execution_handler._stop_button_active:
            self._execution_handler._revert_button_to_send_state()

        # Switch to new tab
        self._active_tab_id = tab_id
        self._tab_bar.set_active_tab(tab_id)

        # Restore state from selected tab
        if tab_id in self._tabs:
            state = self._tabs[tab_id]

            # Switch to tab's handler
            if state.execution_handler:
                self._execution_handler = state.execution_handler

            self._restore_state(state)

            # Restore button state for this tab's handler
            if self._execution_handler._stop_button_active:
                self._execution_handler._transform_button_to_stop(
                    is_alt_enter=(self._execution_handler._stop_button_active == "alt")
                )

            # If handler is streaming, sync accumulated content to restored tree/bubbles
            if self._execution_handler._is_streaming and self._execution_handler._streaming_accumulated:
                if self._conversation_tree and self._execution_handler._pending_assistant_node_id:
                    node = self._conversation_tree.get_node(self._execution_handler._pending_assistant_node_id)
                    if node:
                        node.content = self._execution_handler._streaming_accumulated
                self._rebuild_message_bubbles_from_tree()

            # If handler has pending result (completed while inactive), process it now
            if self._execution_handler._pending_result:
                self._execution_handler._process_pending_result()

    def _on_tab_close_requested(self, tab_id: str):
        """Handle tab close request."""
        if self._tab_bar.get_tab_count() <= 1:
            self._reset_to_initial_state()
            return

        # Get the tab being closed
        closing_state = self._tabs.get(tab_id)

        # If closing active tab, switch to another first
        if tab_id == self._active_tab_id:
            tab_ids = self._tab_bar.get_tab_ids()
            current_idx = tab_ids.index(tab_id)
            new_idx = current_idx - 1 if current_idx > 0 else current_idx + 1
            new_tab_id = tab_ids[new_idx]
            self._on_tab_selected(new_tab_id)

        # Clean up the closed tab's handler
        if closing_state and closing_state.execution_handler:
            handler = closing_state.execution_handler
            # Cancel any active execution
            if handler._current_execution_id:
                handler.stop_execution()
            # Disconnect signals to avoid memory leaks
            handler.disconnect_all_signals()

        self._tabs.pop(tab_id, None)
        self._tab_bar.remove_tab(tab_id)
        self._update_tab_bar_visibility()

    # --- Execution (delegated to ExecutionHandler) ---

    def _expand_output_section(self):
        """Expand output section - add to layout if first time."""
        if not self._output_section_shown:
            # First time showing output
            self.sections_layout.addWidget(self.output_section)
            self.output_edit.installEventFilter(self)
            self._output_section_shown = True
            self.output_edit.setVisible(True)
            self.output_header.set_collapsed(False)
            self._scroll_to_bottom()
        elif not self.output_edit.isVisible():
            # Already in layout, just expand
            self.output_edit.setVisible(True)
            self.output_header.set_collapsed(False)
            self.save_section_state("output_collapsed", False)
            self._scroll_to_bottom()

    def _on_send_copy(self):
        """Ctrl+Enter: Send, copy result to clipboard, close window."""
        self._sync_all_outputs_to_versions()
        message = self.input_edit.toPlainText()
        has_content = bool(message.strip()) or bool(self._message_images)
        if not has_content:
            self.close()
            return
        self._execution_handler.execute_with_message(message, keep_open=False)

    def _on_send_show(self):
        """Alt+Enter: Send, show result in window, stay open. Or regenerate."""
        # Check if in regenerate mode first
        if self._is_regenerate_mode():
            self._regenerate_last_output()
            return

        self._sync_all_outputs_to_versions()
        message = self.input_edit.toPlainText()
        has_content = bool(message.strip()) or bool(self._message_images)
        if not has_content:
            return
        self._execution_handler.execute_with_message(message, keep_open=True)

    def _is_regenerate_mode(self) -> bool:
        """Check if dialog is in regenerate mode (can regenerate last output)."""
        return self._conversation_manager.is_regenerate_mode()

    def _sync_ui_to_conversation_turns(self):
        """Sync current UI text/images back to conversation turn data."""
        if not self._conversation_turns:
            return

        # Sync turn 1 from main input section
        turn1 = self._conversation_turns[0]
        turn1.message_text = self.input_edit.toPlainText()
        turn1.message_images = list(self._message_images)

        # Sync subsequent turns from dynamic reply sections
        for i, section in enumerate(self._dynamic_sections):
            turn_idx = i + 1  # Turn 2 is at index 1, etc.
            if turn_idx < len(self._conversation_turns):
                turn = self._conversation_turns[turn_idx]
                turn.message_text = section.text_edit.toPlainText()
                turn.message_images = list(section.turn_images)

    def set_history_service(self, service) -> None:
        """Set the history service for conversation persistence."""
        self._history_service = service

    def restore_from_history(self, entry_id: str) -> bool:
        """Restore full conversation state from history entry.

        Args:
            entry_id: ID of the history entry to restore

        Returns:
            True if restoration successful, False otherwise
        """
        if not self._history_service:
            return False

        conv_data = self._history_service.get_conversation_data(entry_id)
        if not conv_data:
            return False

        # Clear current state
        self._clear_dynamic_sections()
        if self._output_section_shown:
            self.sections_layout.removeWidget(self.output_section)
            self.output_section.setParent(None)
            self._output_section_shown = False

        # Restore context
        self._current_images = self._history_service.load_images_from_paths(conv_data.context_image_paths)
        self._rebuild_image_chips()
        self.context_text_edit.blockSignals(True)
        self.context_text_edit.setPlainText(conv_data.context_text)
        self.context_text_edit.blockSignals(False)
        self._last_context_text = conv_data.context_text

        # Clear undo stacks since we're loading saved state
        self._context_undo_stack.clear()
        self._context_redo_stack.clear()
        self._input_undo_stack.clear()
        self._input_redo_stack.clear()
        self._output_undo_stack.clear()
        self._output_redo_stack.clear()

        # Restore conversation turns
        self._conversation_turns.clear()
        self._current_turn_number = 0

        for serialized_turn in conv_data.turns:
            turn_images = self._history_service.load_images_from_paths(serialized_turn.message_image_paths)
            turn = ConversationTurn(
                turn_number=serialized_turn.turn_number,
                message_text=serialized_turn.message_text,
                message_images=turn_images,
                output_text=serialized_turn.output_text,
                is_complete=serialized_turn.is_complete,
                output_versions=list(serialized_turn.output_versions),
                current_version_index=serialized_turn.current_version_index,
            )
            self._conversation_turns.append(turn)
            self._current_turn_number = max(self._current_turn_number, turn.turn_number)

        # Restore tree-based conversation if available
        tree = self._history_service.deserialize_tree_nodes(conv_data)
        if tree:
            self._conversation_tree = tree
            self._rebuild_message_bubbles_from_tree()
        else:
            # Legacy: Rebuild UI from turns
            self._restore_conversation_ui()

        # Store entry ID for future updates
        self._history_entry_id = entry_id

        # Update buttons
        self._update_undo_redo_buttons()
        self._update_send_buttons_state()
        self._update_delete_button_visibility()

        return True

    def _restore_conversation_ui(self):
        """Rebuild UI sections from conversation turns."""
        if not self._conversation_turns:
            return

        # First turn goes to main input/output sections
        turn1 = self._conversation_turns[0]
        self._message_images = list(turn1.message_images)
        self._rebuild_message_image_chips()
        self.input_edit.blockSignals(True)
        self.input_edit.setPlainText(turn1.message_text)
        self.input_edit.blockSignals(False)
        self._last_input_text = turn1.message_text

        if turn1.is_complete and turn1.output_text:
            self._expand_output_section()
            self.output_edit.blockSignals(True)
            self.output_edit.setPlainText(turn1.output_text)
            self.output_edit.blockSignals(False)
            self._last_output_text = turn1.output_text
            if turn1.output_versions:
                self.output_header.set_version_info(turn1.current_version_index + 1, len(turn1.output_versions))

        # Subsequent turns create output sections only
        for turn in self._conversation_turns[1:]:
            if turn.is_complete and turn.output_text:
                output_section = self._conversation_manager.create_dynamic_output_section(turn.turn_number)
                output_section.text_edit.setPlainText(turn.output_text)
                output_section.last_text = turn.output_text
                if turn.output_versions:
                    output_section.header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))
                self._output_sections.append(output_section)
                self.sections_layout.addWidget(output_section)
                output_section.text_edit.installEventFilter(self)

        self._renumber_sections()

    def _save_to_history(self):
        """Save or update conversation state to history service."""
        if not self._history_service:
            return

        # Don't save if no completed turns (legacy) and no completed tree nodes
        has_completed_turns = any(t.is_complete for t in self._conversation_turns)
        has_completed_tree = (
            self._conversation_tree
            and not self._conversation_tree.is_empty()
            and any(n.role == "assistant" and n.content for n in self._conversation_tree.nodes.values())
        )
        if not has_completed_turns and not has_completed_tree:
            return

        # Sync UI to turn data before saving
        self._sync_ui_to_conversation_turns()
        self._sync_all_outputs_to_versions()
        self._sync_bubbles_to_tree()

        prompt_id = self.menu_item.data.get("prompt_id") if self.menu_item.data else None
        prompt_name = self.menu_item.data.get("prompt_name") if self.menu_item.data else None

        if self._history_entry_id:
            # Update existing entry
            self._history_service.update_conversation_entry(
                self._history_entry_id,
                self._conversation_turns,
                self.context_text_edit.toPlainText(),
                self._current_images,
                conversation_tree=self._conversation_tree,
            )
        else:
            # Create new entry
            self._history_entry_id = self._history_service.add_conversation_entry(
                self._conversation_turns,
                self.context_text_edit.toPlainText(),
                self._current_images,
                prompt_id=prompt_id,
                prompt_name=prompt_name,
                conversation_tree=self._conversation_tree,
            )

    def _regenerate_last_output(self):
        """Regenerate the last AI output by re-executing the last message."""
        # Handle tree-based regeneration
        if self._conversation_tree and not self._conversation_tree.is_empty():
            self._regenerate_from_tree()
            return

        # Legacy: linear conversation handling
        if not self._conversation_turns:
            return

        last_turn = self._conversation_turns[-1]
        if not last_turn.is_complete:
            return

        turn_number = last_turn.turn_number

        # Save current version's edited text and undo state
        if last_turn.output_versions:
            if turn_number == 1 or not self._output_sections:
                current_output_text = self.output_edit.toPlainText()
                last_turn.output_versions[last_turn.current_version_index] = current_output_text
                self._save_current_version_undo_state(last_turn)
            else:
                section = self._output_sections[-1]
                current_output_text = section.text_edit.toPlainText()
                last_turn.output_versions[last_turn.current_version_index] = current_output_text
                self._save_dynamic_version_undo_state(section, last_turn)

        # Create copies of version history
        existing_versions = list(last_turn.output_versions)
        existing_version_undo_states = list(last_turn.version_undo_states)

        # Create a NEW version placeholder BEFORE execution
        # This isolates the previous version completely
        from modules.gui.prompt_execute_dialog.data import OutputVersionState

        existing_versions.append("")  # Placeholder for new output
        existing_version_undo_states.append(OutputVersionState(undo_stack=[], redo_stack=[], last_text=""))
        new_version_index = len(existing_versions) - 1

        # Sync UI edits to conversation turns before removing last turn
        self._sync_ui_to_conversation_turns()

        # Read current text from UI (may have been edited by user)
        if self._dynamic_sections and turn_number > 1:
            section = self._dynamic_sections[-1]
            message_text = section.text_edit.toPlainText()
        else:
            message_text = self.input_edit.toPlainText()

        # Remove last turn from history - will be re-added by _execute_with_message
        self._conversation_turns.pop()

        # For turn 1, set to 0 so _execute_with_message increments to 1
        # For turn 2+, keep current turn number so it reuses existing output section
        if turn_number == 1:
            self._current_turn_number = 0

        # Store version history with NEW version already added
        self._pending_version_history = existing_versions
        self._pending_version_undo_states = existing_version_undo_states
        self._pending_version_index = new_version_index  # Point to NEW version
        self._pending_is_regeneration = True  # Flag for result handler

        self._execution_handler.execute_with_message(message_text, keep_open=True, regenerate=True)

    def _regenerate_from_tree(self):
        """Regenerate response using tree-based branching."""
        if not self._conversation_tree:
            return

        leaf = self._conversation_tree.get_current_leaf()
        if not leaf or leaf.role != "assistant":
            return

        # Find the parent user message
        user_node_id = leaf.parent_id
        if not user_node_id:
            return

        user_node = self._conversation_tree.get_node(user_node_id)
        if not user_node:
            return

        # Sync current bubble content to tree
        self._sync_bubbles_to_tree()

        # Create new assistant node as sibling branch
        new_assistant = create_node(
            role="assistant",
            content="",
            parent_id=user_node_id,
        )
        self._conversation_tree.add_node(new_assistant)

        # Update current path to new branch
        try:
            old_idx = self._conversation_tree.current_path.index(leaf.node_id)
            self._conversation_tree.current_path = self._conversation_tree.current_path[:old_idx]
            self._conversation_tree.current_path.append(new_assistant.node_id)
        except ValueError:
            self._conversation_tree.current_path.append(new_assistant.node_id)

        # Set pending node for execution handler
        self._execution_handler._pending_user_node_id = user_node_id
        self._execution_handler._pending_assistant_node_id = new_assistant.node_id

        self._pending_is_regeneration = True

        # Rebuild bubbles to show the new branch
        self._rebuild_message_bubbles_from_tree()

        # Execute using tree-based conversation data
        self._execution_handler.execute_with_message(
            user_node.content, keep_open=True, regenerate=True
        )

    def _create_dynamic_output_section(self, turn_number: int) -> QWidget:
        """Create output section for a conversation turn."""
        return self._conversation_manager.create_dynamic_output_section(turn_number)

    # --- Event handling ---

    def _trigger_send_from_text_edit(self):
        """Trigger send action when Enter is pressed in a text edit."""
        if self._is_regenerate_mode():
            self._regenerate_last_output()
            return

        has_content = bool(self.input_edit.toPlainText().strip()) or bool(self._message_images)
        if has_content and not self._waiting_for_result:
            self._on_send_show()

    def keyPressEvent(self, event):
        """Handle key press events."""
        key = event.key()
        modifiers = event.modifiers()

        # Check if message has content (text or images)
        has_content = bool(self.input_edit.toPlainText().strip()) or bool(self._message_images)

        # Check if in regenerate mode
        is_regenerate = self._is_regenerate_mode()

        # Ctrl+Enter: Send, copy result to clipboard, close window
        # Or stop execution if already executing (ctrl button in stop mode)
        if key in (Qt.Key_Return, Qt.Key_Enter) and (modifiers & Qt.ControlModifier):
            if self._waiting_for_result and self._execution_handler._stop_button_active == "ctrl":
                self._execution_handler.stop_execution()
            elif has_content and not self._waiting_for_result:
                self._on_send_copy()
            event.accept()
            return

        # Alt+Enter: Send & show, OR regenerate, OR stop execution
        if key in (Qt.Key_Return, Qt.Key_Enter) and (modifiers & Qt.AltModifier):
            if self._waiting_for_result and self._execution_handler._stop_button_active == "alt":
                self._execution_handler.stop_execution()
            elif is_regenerate and not self._waiting_for_result:
                self._regenerate_last_output()
            elif has_content and not self._waiting_for_result:
                self._on_send_show()
            event.accept()
            return

        # Plain Enter: Send & show (chat-like behavior, fallback for non-filtered events)
        if key in (Qt.Key_Return, Qt.Key_Enter) and not (modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.ShiftModifier)):
            if self._waiting_for_result and self._execution_handler._stop_button_active == "alt":
                self._execution_handler.stop_execution()
            elif is_regenerate and not self._waiting_for_result:
                self._regenerate_last_output()
            elif has_content and not self._waiting_for_result:
                self._on_send_show()
            event.accept()
            return

        # Escape to close
        if key == Qt.Key_Escape:
            self.close()
            return

        super().keyPressEvent(event)

    def eventFilter(self, obj, event):
        """Filter events to intercept key presses on text edits."""
        if event.type() == QEvent.KeyPress:
            key = event.key()
            modifiers = event.modifiers()

            # Handle Enter key on text edits (chat-like behavior)
            if key in (Qt.Key_Return, Qt.Key_Enter):
                # Shift+Enter: Insert newline (let default handle it)
                if modifiers & Qt.ShiftModifier:
                    return False

                # Plain Enter: Trigger send (like in chat apps)
                # Don't consume if there are other modifiers (Ctrl, Alt handled elsewhere)
                if not (modifiers & (Qt.ControlModifier | Qt.AltModifier)):
                    self._trigger_send_from_text_edit()
                    return True

            # Ctrl+Z for undo
            if key == Qt.Key_Z and (modifiers & Qt.ControlModifier):
                if modifiers & Qt.ShiftModifier:
                    # Ctrl+Shift+Z for redo
                    if obj == self.context_text_edit:
                        self._redo_context()
                    elif obj == self.input_edit:
                        self._redo_input()
                    elif obj == self.output_edit:
                        self._redo_output()
                    else:
                        # Check dynamic sections
                        for section in self._dynamic_sections + self._output_sections:
                            if obj == section.text_edit:
                                self._redo_dynamic_section(section)
                                break
                else:
                    # Ctrl+Z for undo
                    if obj == self.context_text_edit:
                        self._undo_context()
                    elif obj == self.input_edit:
                        self._undo_input()
                    elif obj == self.output_edit:
                        self._undo_output()
                    else:
                        # Check dynamic sections
                        for section in self._dynamic_sections + self._output_sections:
                            if obj == section.text_edit:
                                self._undo_dynamic_section(section)
                                break
                return True  # Event handled

            # Ctrl+Y for redo (alternative)
            if key == Qt.Key_Y and (modifiers & Qt.ControlModifier):
                if obj == self.context_text_edit:
                    self._redo_context()
                elif obj == self.input_edit:
                    self._redo_input()
                elif obj == self.output_edit:
                    self._redo_output()
                else:
                    # Check dynamic sections
                    for section in self._dynamic_sections + self._output_sections:
                        if obj == section.text_edit:
                            self._redo_dynamic_section(section)
                            break
                return True

            # Ctrl+V for paste image (if clipboard has image)
            if key == Qt.Key_V and (modifiers & Qt.ControlModifier):
                if obj == self.context_text_edit:
                    if self._paste_image_from_clipboard():
                        return True  # Event handled, don't paste as text
                elif obj == self.input_edit and self._paste_image_to_message():
                    return True  # Event handled, don't paste as text

        return super().eventFilter(obj, event)
