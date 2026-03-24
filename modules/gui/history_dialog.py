"""History dialog for displaying execution history."""

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from core.interfaces import ClipboardManager
from core.models import HistoryEntry, HistoryEntryType
from modules.gui.icons import ICON_COLOR_NORMAL, create_icon_pixmap
from modules.gui.shared.base_dialog import BaseDialog
from modules.gui.shared.widgets import NoScrollComboBox
from modules.gui.shared.context_widgets import IconButton
from modules.gui.shared.theme import (
    COLOR_BORDER,
    COLOR_BUTTON_BG,
    COLOR_BUTTON_HOVER,
    COLOR_ERROR_BG,
    COLOR_ERROR_BG_HOVER,
    COLOR_ERROR_BORDER,
    COLOR_TEXT,
    COLOR_TEXT_HINT,
    COLOR_TEXT_LIGHT,
    COLOR_TEXT_SECONDARY,
    COLOR_TEXT_WHITE,
    COMBOBOX_STYLE,
    ICON_BTN_STYLE,
    SCROLL_CONTENT_MARGINS,
    SCROLL_CONTENT_SPACING,
    SMALL_DIALOG_SIZE,
    SMALL_MIN_DIALOG_SIZE,
    create_singleton_dialog_manager,
)

_show_dialog = create_singleton_dialog_manager()


class ClickableLabel(QLabel):
    clicked = Signal()

    def __init__(self, text: str = "", parent: QWidget | None = None):
        super().__init__(text, parent)
        self.setCursor(Qt.PointingHandCursor)
        self._hover = False
        self._clickable = True

    def set_clickable(self, clickable: bool):
        self._clickable = clickable
        self.setCursor(Qt.PointingHandCursor if clickable else Qt.ArrowCursor)

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton and self._clickable:
            self.clicked.emit()
        super().mousePressEvent(event)

    def enterEvent(self, event):
        if self._clickable:
            self._hover = True
            self.setStyleSheet(self.styleSheet() + "QLabel { text-decoration: underline; }")
        super().enterEvent(event)

    def leaveEvent(self, event):
        if self._clickable:
            self._hover = False
            style = self.styleSheet().replace("QLabel { text-decoration: underline; }", "")
            self.setStyleSheet(style)
        super().leaveEvent(event)


def show_history_dialog(
    history_service,
    clipboard_manager: ClipboardManager | None = None,
    prompt_store_service=None,
    notification_manager=None,
    on_open_conversation=None,
):
    def create_dialog():
        dialog = HistoryDialog(
            history_service,
            clipboard_manager=clipboard_manager,
            prompt_store_service=prompt_store_service,
            notification_manager=notification_manager,
        )
        if on_open_conversation:
            dialog.open_conversation_requested.connect(on_open_conversation)
        return dialog

    _show_dialog("history_dialog", create_dialog)


class HistoryEntryWidget(QWidget):
    """Widget displaying a single history entry row."""

    conversation_clicked = Signal(str)  # Emits entry_id when conversation is clicked

    _entry_style = f"""
        QWidget#historyEntry {{
            background-color: {COLOR_BUTTON_BG};
            border: 1px solid {COLOR_BORDER};
            border-radius: 8px;
            padding: 4px;
        }}
    """

    _entry_hover_style = f"""
        QWidget#historyEntry {{
            background-color: {COLOR_BUTTON_HOVER};
            border: 1px solid {COLOR_BORDER};
            border-radius: 8px;
            padding: 4px;
        }}
    """

    _entry_error_style = f"""
        QWidget#historyEntry {{
            background-color: {COLOR_ERROR_BG};
            border: 1px solid {COLOR_ERROR_BORDER};
            border-radius: 8px;
            padding: 4px;
        }}
    """

    _entry_error_hover_style = f"""
        QWidget#historyEntry {{
            background-color: {COLOR_ERROR_BG_HOVER};
            border: 1px solid {COLOR_ERROR_BORDER};
            border-radius: 8px;
            padding: 4px;
        }}
    """

    _turn_count_style = f"""
        QLabel {{
            color: {COLOR_TEXT_SECONDARY};
            font-size: 11px;
            background: transparent;
        }}
    """

    _label_style = f"""
        QLabel {{
            color: {COLOR_TEXT};
            font-size: 12px;
            background: transparent;
        }}
    """

    _timestamp_style = f"""
        QLabel {{
            color: {COLOR_TEXT_SECONDARY};
            font-size: 11px;
            background: transparent;
        }}
    """

    _prompt_name_style = f"""
        QLabel {{
            color: {COLOR_TEXT_WHITE};
            font-size: 11px;
            font-weight: bold;
            background: transparent;
        }}
    """

    _section_label_style = f"""
        QLabel {{
            color: {COLOR_TEXT_SECONDARY};
            font-size: 11px;
            background: transparent;
        }}
    """

    _truncated_text_style = f"""
        QLabel {{
            color: {COLOR_TEXT_LIGHT};
            font-size: 11px;
            font-family: monospace;
            background: transparent;
        }}
    """

    _truncated_text_empty_style = f"""
        QLabel {{
            color: {COLOR_TEXT_HINT};
            font-size: 11px;
            font-style: italic;
            background: transparent;
        }}
    """

    def __init__(
        self,
        entry: HistoryEntry,
        clipboard_manager: ClipboardManager | None = None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.entry = entry
        self.clipboard_manager = clipboard_manager
        self._is_error = not entry.success

        self._input_text_label: ClickableLabel | None = None
        self._output_text_label: ClickableLabel | None = None

        self.setObjectName("historyEntry")
        self.setAttribute(Qt.WA_StyledBackground, True)
        self._apply_style()

        self._setup_ui()

    def _apply_style(self):
        if self._is_error:
            self.setStyleSheet(self._entry_error_style)
        else:
            self.setStyleSheet(self._entry_style)

    def _truncate_text(self, text: str | None, max_chars: int = 100) -> tuple[str, bool]:
        if not text:
            return "(empty)", False
        clean = text.replace("\n", " ").strip()
        if len(clean) <= max_chars:
            return clean, False
        return clean[:max_chars] + "...", True

    def _setup_ui(self):
        main_layout = QVBoxLayout(self)
        main_layout.setContentsMargins(8, 6, 8, 6)
        main_layout.setSpacing(4)

        # Header row
        header_row = QHBoxLayout()
        header_row.setSpacing(8)

        type_icon_name = "mic" if self.entry.entry_type == HistoryEntryType.SPEECH else "message-square-reply"
        type_icon = QLabel()
        type_icon.setPixmap(create_icon_pixmap(type_icon_name, ICON_COLOR_NORMAL, 16))
        type_icon.setFixedSize(16, 16)
        header_row.addWidget(type_icon)

        if self.entry.is_conversation:
            conversation_icon = QLabel()
            conversation_icon.setPixmap(create_icon_pixmap("message-square-share", "#6ba3ff", 16))
            conversation_icon.setFixedSize(16, 16)
            conversation_icon.setToolTip("From conversation - showing last message only")
            header_row.addWidget(conversation_icon)

        if self._is_error:
            error_icon = QLabel()
            error_icon.setPixmap(create_icon_pixmap("circle-alert", "#ff6b6b", 16))
            error_icon.setFixedSize(16, 16)
            error_icon.setToolTip(self.entry.error or "Error")
            header_row.addWidget(error_icon)

        timestamp_label = QLabel(self.entry.timestamp)
        timestamp_label.setStyleSheet(self._timestamp_style)
        header_row.addWidget(timestamp_label)

        if self.entry.prompt_name:
            prompt_name_label = QLabel(self.entry.prompt_name)
            prompt_name_label.setStyleSheet(self._prompt_name_style)
            header_row.addWidget(prompt_name_label)

        # Show turn count for conversation entries
        if self.entry.conversation_data and self.entry.conversation_data.turns:
            turn_count = len(self.entry.conversation_data.turns)
            turn_label = QLabel(f"({turn_count} turn{'s' if turn_count > 1 else ''})")
            turn_label.setStyleSheet(self._turn_count_style)
            header_row.addWidget(turn_label)

        header_row.addStretch()

        if self.entry.conversation_data:
            open_btn = IconButton("preview", size=16)
            open_btn.setStyleSheet(ICON_BTN_STYLE)
            open_btn.setToolTip("Open conversation")
            open_btn.setCursor(Qt.PointingHandCursor)
            open_btn.clicked.connect(self._on_open_conversation)
            header_row.addWidget(open_btn)

        main_layout.addLayout(header_row)

        # Input row
        full_input = self._get_full_input_content()
        has_input = bool(full_input)
        input_row = QHBoxLayout()
        input_row.setSpacing(8)

        input_label = QLabel("Input:")
        input_label.setStyleSheet(self._section_label_style)
        input_row.addWidget(input_label)

        input_text, input_truncated = self._truncate_text(full_input)
        input_text_label = ClickableLabel(input_text)
        input_text_label.setWordWrap(True)
        input_text_label.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        if has_input:
            input_text_label.setStyleSheet(self._truncated_text_style)
            input_text_label.clicked.connect(self._preview_input)
            if input_truncated:
                input_text_label.setToolTip(full_input)
        else:
            input_text_label.setStyleSheet(self._truncated_text_empty_style)
            input_text_label.set_clickable(False)
        self._input_text_label = input_text_label
        input_row.addWidget(input_text_label)

        self._input_copy_btn = IconButton("copy", size=16)
        self._input_copy_btn.setStyleSheet(ICON_BTN_STYLE)
        self._input_copy_btn.setToolTip("Copy input")
        self._input_copy_btn.setEnabled(has_input)
        self._input_copy_btn.setCursor(Qt.PointingHandCursor if has_input else Qt.ArrowCursor)
        self._input_copy_btn.clicked.connect(self._copy_input)
        input_row.addWidget(self._input_copy_btn)

        main_layout.addLayout(input_row)

        # Output row
        full_output = self._get_full_output_content()
        has_output = bool(full_output)
        output_row = QHBoxLayout()
        output_row.setSpacing(8)

        output_label = QLabel("Output:")
        output_label.setStyleSheet(self._section_label_style)
        output_row.addWidget(output_label)

        output_text, output_truncated = self._truncate_text(full_output)
        output_text_label = ClickableLabel(output_text)
        output_text_label.setWordWrap(True)
        output_text_label.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Preferred)
        if has_output:
            output_text_label.setStyleSheet(self._truncated_text_style)
            output_text_label.clicked.connect(self._preview_output)
            if output_truncated:
                output_text_label.setToolTip(full_output)
        else:
            output_text_label.setStyleSheet(self._truncated_text_empty_style)
            output_text_label.set_clickable(False)
        self._output_text_label = output_text_label
        output_row.addWidget(output_text_label)

        self._output_copy_btn = IconButton("copy", size=16)
        self._output_copy_btn.setStyleSheet(ICON_BTN_STYLE)
        self._output_copy_btn.setToolTip("Copy output")
        self._output_copy_btn.setEnabled(has_output)
        self._output_copy_btn.setCursor(Qt.PointingHandCursor if has_output else Qt.ArrowCursor)
        self._output_copy_btn.clicked.connect(self._copy_output)
        output_row.addWidget(self._output_copy_btn)

        main_layout.addLayout(output_row)

    def _get_full_input_content(self) -> str | None:
        if self.entry.conversation_data:
            conv_data = self.entry.conversation_data
            if conv_data.turns:
                last_turn = conv_data.turns[-1]
                if last_turn.message_text:
                    return last_turn.message_text
                if last_turn.message_image_paths:
                    return "(image)"
            if conv_data.nodes and conv_data.current_path:
                nodes_by_id = {node.node_id: node for node in conv_data.nodes}
                for node_id in reversed(conv_data.current_path):
                    node = nodes_by_id.get(node_id)
                    if node and node.role == "user":
                        if node.content:
                            return node.content
                        if node.image_paths:
                            return "(image)"
                        break
            return None
        return self.entry.input_content if self.entry.input_content else None

    def _get_full_output_content(self) -> str | None:
        if self.entry.conversation_data:
            conv_data = self.entry.conversation_data
            if conv_data.turns:
                last_turn = conv_data.turns[-1]
                if last_turn.output_text:
                    return last_turn.output_text
            if conv_data.nodes and conv_data.current_path:
                nodes_by_id = {node.node_id: node for node in conv_data.nodes}
                for node_id in reversed(conv_data.current_path):
                    node = nodes_by_id.get(node_id)
                    if node and node.role == "assistant" and node.content:
                        return node.content
            return None
        return self.entry.output_content if self.entry.output_content else None

    def _copy_input(self):
        from modules.gui.shared.icon_confirmation import flash_confirmation

        full_content = self._get_full_input_content()
        if full_content and self.clipboard_manager:
            self.clipboard_manager.set_content(full_content)
            flash_confirmation(self._input_copy_btn)

    def _copy_output(self):
        from modules.gui.shared.icon_confirmation import flash_confirmation

        full_content = self._get_full_output_content()
        if full_content and self.clipboard_manager:
            self.clipboard_manager.set_content(full_content)
            flash_confirmation(self._output_copy_btn)

    def _preview_input(self):
        full_content = self._get_full_input_content()
        if full_content:
            from modules.gui.text_preview_dialog import show_preview_dialog

            show_preview_dialog(
                "Input Content",
                full_content,
                clipboard_manager=self.clipboard_manager,
            )

    def _preview_output(self):
        full_content = self._get_full_output_content()
        if full_content:
            from modules.gui.text_preview_dialog import show_preview_dialog

            show_preview_dialog(
                "Output Content",
                full_content,
                clipboard_manager=self.clipboard_manager,
            )

    def _on_open_conversation(self):
        """Handle open conversation button click."""
        self.conversation_clicked.emit(self.entry.id)

    def enterEvent(self, event):
        if self._is_error:
            self.setStyleSheet(self._entry_error_hover_style)
        else:
            self.setStyleSheet(self._entry_hover_style)
        super().enterEvent(event)

    def leaveEvent(self, event):
        self._apply_style()
        super().leaveEvent(event)


class HistoryDialog(BaseDialog):
    """Dialog for displaying execution history with pagination."""

    STATE_KEY = "history_dialog"
    DEFAULT_SIZE = SMALL_DIALOG_SIZE
    MIN_SIZE = SMALL_MIN_DIALOG_SIZE

    history_changed = Signal()
    open_conversation_requested = Signal(str, str)  # entry_id, prompt_id

    VALID_PAGE_SIZES = [10, 25, 50]
    DEFAULT_PAGE_SIZE = 10

    def __init__(
        self,
        history_service,
        parent=None,
        clipboard_manager: ClipboardManager | None = None,
        prompt_store_service=None,
        notification_manager=None,
    ):
        super().__init__(parent)
        self.setWindowTitle("Execution History")

        self.history_service = history_service
        self.clipboard_manager = clipboard_manager
        self.prompt_store_service = prompt_store_service
        self.notification_manager = notification_manager

        self.current_page = 0
        self.page_size = self.DEFAULT_PAGE_SIZE

        self.history_changed.connect(self._load_page)

        self._setup_ui()
        self.apply_dialog_styles()
        self._restore_state()
        self._load_page()

        if self.history_service:
            self.history_service.add_change_callback(self._on_history_changed)

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)
        layout.setSpacing(8)

        # Scroll area for entries
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.scroll_area.setFrameShape(QFrame.NoFrame)

        self.entries_container = QWidget()
        self.entries_layout = QVBoxLayout(self.entries_container)
        self.entries_layout.setContentsMargins(*SCROLL_CONTENT_MARGINS)
        self.entries_layout.setSpacing(SCROLL_CONTENT_SPACING)
        self.entries_layout.addStretch()

        self.scroll_area.setWidget(self.entries_container)
        layout.addWidget(self.scroll_area)

        # Pagination controls
        pagination = QHBoxLayout()
        pagination.setSpacing(8)

        page_size_label = QLabel("Page size:")
        page_size_label.setStyleSheet("QLabel { color: #888888; }")
        pagination.addWidget(page_size_label)

        self.page_size_combo = NoScrollComboBox()
        self.page_size_combo.setStyleSheet(COMBOBOX_STYLE)
        for size in self.VALID_PAGE_SIZES:
            self.page_size_combo.addItem(str(size), size)
        self.page_size_combo.currentIndexChanged.connect(self._on_page_size_changed)
        pagination.addWidget(self.page_size_combo)

        pagination.addStretch()

        self.page_label = QLabel("Page 1 of 1")
        self.page_label.setStyleSheet("QLabel { color: #888888; }")
        pagination.addWidget(self.page_label)

        pagination.addStretch()

        self.prev_btn = QPushButton("< Prev")
        self.prev_btn.clicked.connect(self._prev_page)
        pagination.addWidget(self.prev_btn)

        self.next_btn = QPushButton("Next >")
        self.next_btn.clicked.connect(self._next_page)
        pagination.addWidget(self.next_btn)

        layout.addLayout(pagination)

    def _restore_state(self):
        self.restore_geometry_from_state()

        saved_page_size = self._ui_state.get(f"{self.STATE_KEY}.page_size", self.DEFAULT_PAGE_SIZE)
        if saved_page_size in self.VALID_PAGE_SIZES:
            self.page_size = saved_page_size
            index = self.VALID_PAGE_SIZES.index(saved_page_size)
            self.page_size_combo.setCurrentIndex(index)
        else:
            self.page_size = self.DEFAULT_PAGE_SIZE

    def _on_page_size_changed(self, index: int):
        new_size = self.page_size_combo.itemData(index)
        if new_size and new_size != self.page_size:
            self.page_size = new_size
            self._ui_state.set(f"{self.STATE_KEY}.page_size", new_size)
            self.current_page = 0
            self._load_page()

    def _on_history_changed(self):
        self.history_changed.emit()

    def _load_page(self):
        all_entries = self.history_service.get_history() if self.history_service else []
        total = len(all_entries)
        total_pages = max(1, (total + self.page_size - 1) // self.page_size)

        self.current_page = min(self.current_page, total_pages - 1)

        start = self.current_page * self.page_size
        end = start + self.page_size
        page_entries = all_entries[start:end]

        self._clear_entries()

        if page_entries:
            for entry in page_entries:
                widget = HistoryEntryWidget(entry, self.clipboard_manager)
                # Connect conversation click signal
                if entry.conversation_data:
                    widget.conversation_clicked.connect(self._on_conversation_clicked)
                self.entries_layout.insertWidget(self.entries_layout.count() - 1, widget)
        else:
            self._show_empty_state()

        self.prev_btn.setEnabled(self.current_page > 0)
        self.next_btn.setEnabled(self.current_page < total_pages - 1)
        self.page_label.setText(f"Page {self.current_page + 1} of {total_pages}")

    def _clear_entries(self):
        while self.entries_layout.count() > 1:
            item = self.entries_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

    def _show_empty_state(self):
        empty_label = QLabel("No execution history yet")
        empty_label.setStyleSheet("QLabel { color: #666666; font-size: 12px; padding: 20px; }")
        empty_label.setAlignment(Qt.AlignCenter)
        self.entries_layout.insertWidget(0, empty_label)

    def _prev_page(self):
        if self.current_page > 0:
            self.current_page -= 1
            self._load_page()

    def _next_page(self):
        all_entries = self.history_service.get_history() if self.history_service else []
        total = len(all_entries)
        total_pages = max(1, (total + self.page_size - 1) // self.page_size)

        if self.current_page < total_pages - 1:
            self.current_page += 1
            self._load_page()

    def _on_conversation_clicked(self, entry_id: str):
        """Handle conversation entry click - open in prompt execute dialog."""
        entry = self.history_service.get_entry_by_id(entry_id) if self.history_service else None
        if not entry or not entry.conversation_data:
            return

        prompt_id = entry.conversation_data.prompt_id or ""
        self._open_conversation(entry_id, prompt_id)

    def _open_conversation(self, entry_id: str, prompt_id: str):
        """Open conversation dialog from history entry."""
        import logging

        logger = logging.getLogger(__name__)

        if not self.prompt_store_service:
            logger.warning("Cannot open conversation: prompt_store_service is None")
            return

        if not entry_id:
            logger.warning("Cannot open conversation: entry_id is empty")
            return

        menu_item = None
        all_items = self.prompt_store_service.get_all_available_prompts()

        if prompt_id:
            for item in all_items:
                item_prompt_id = item.data.get("prompt_id") if item.data else None
                if item_prompt_id == prompt_id:
                    menu_item = item
                    break

        if not menu_item and all_items:
            menu_item = all_items[0]
            logger.info(f"Using fallback prompt for conversation {entry_id}")

        if not menu_item:
            logger.warning("Cannot open conversation: no prompts available")
            return

        context_manager = getattr(self.prompt_store_service, "context_manager", None)

        from modules.gui.prompt_execute_dialog import show_prompt_execute_dialog

        show_prompt_execute_dialog(
            menu_item,
            lambda item, shift: None,
            prompt_store_service=self.prompt_store_service,
            context_manager=context_manager,
            clipboard_manager=self.clipboard_manager,
            notification_manager=self.notification_manager,
            history_service=self.history_service,
            history_entry_id=entry_id,
        )

    def closeEvent(self, event):
        if self.history_service:
            self.history_service.remove_change_callback(self._on_history_changed)
        super().closeEvent(event)

    def keyPressEvent(self, event):
        if self.handle_escape_key(event):
            return
        super().keyPressEvent(event)
