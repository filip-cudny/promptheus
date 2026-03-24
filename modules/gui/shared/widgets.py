"""Shared widgets for GUI dialogs."""

import base64
import logging
from collections.abc import Callable
from typing import TYPE_CHECKING, Generic, TypeVar

from PySide6.QtCore import QBuffer, QByteArray, Qt, QTimer, Signal

if TYPE_CHECKING:
    from core.context_manager import ContextItem
from PySide6.QtGui import QFont, QImage
from PySide6.QtWidgets import (
    QApplication,
    QComboBox,
    QGraphicsOpacityEffect,
    QHBoxLayout,
    QLabel,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from modules.gui.icons import DISABLED_OPACITY, ICON_COLOR_NORMAL
from modules.gui.shared.context_widgets import IconButton
from modules.gui.shared.theme import (
    COLOR_BORDER,
    COLOR_BUTTON_BG,
    COLOR_BUTTON_HOVER,
    COLOR_TEXT,
    HEADER_ICON_SIZE,
    ICON_BTN_STYLE,
    SECTION_HINT_STYLE,
    SECTION_TITLE_STYLE,
    TOOLTIP_STYLE,
)
from modules.gui.shared.undo_redo import TextEditUndoHelper
from modules.utils.notification_config import is_notification_enabled

logger = logging.getLogger(__name__)


class NoScrollComboBox(QComboBox):
    def wheelEvent(self, event):
        event.ignore()

# Type variable for generic undo/redo manager
T = TypeVar("T")

# Minimum height for text edit widgets in dialogs
TEXT_EDIT_MIN_HEIGHT = 300

# Minimum height for text edits in chat message bubbles (approximately 1 line with padding)
BUBBLE_TEXT_EDIT_MIN_HEIGHT = 36


class CollapsibleSectionHeader(QWidget):
    """Header widget for collapsible sections with title, collapse toggle, and optional buttons."""

    toggle_requested = Signal()
    save_requested = Signal()
    undo_requested = Signal()
    redo_requested = Signal()
    delete_requested = Signal()
    wrap_requested = Signal()
    version_prev_requested = Signal()
    version_next_requested = Signal()
    regenerate_requested = Signal()
    copy_content_requested = Signal()
    render_toggle_requested = Signal()

    def __init__(
        self,
        title: str,
        show_save_button: bool = True,
        show_undo_redo: bool = False,
        show_delete_button: bool = False,
        show_wrap_button: bool = False,
        show_version_nav: bool = False,
        show_regenerate_button: bool = False,
        show_info_button: bool = False,
        show_copy_content_button: bool = False,
        show_render_toggle: bool = False,
        hint_text: str = "",
        badge_widget: QWidget | None = None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._collapsed = False
        self._title = title
        self._has_content = False

        # Ensure transparent background
        self.setAttribute(Qt.WA_StyledBackground, True)
        self.setStyleSheet("background: transparent;")

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 12, 0)  # Minimal vertical margins for compact collapsed headers
        layout.setSpacing(4)

        # Collapse toggle button FIRST (left side)
        self.toggle_btn = IconButton("chevron-down", size=16)
        self.toggle_btn.setToolTip("Collapse section")
        # Remove padding so chevron aligns with text edit border
        self.toggle_btn.setFixedSize(16, 16)
        self.toggle_btn.setStyleSheet("QPushButton { background: transparent; border: none; padding: 0px; }")
        self.toggle_btn.clicked.connect(lambda: self.toggle_requested.emit())
        layout.addWidget(self.toggle_btn)

        # Title label after chevron
        self.title_label = QLabel(title)
        self.title_label.setStyleSheet(SECTION_TITLE_STYLE)
        if not title:
            self.title_label.hide()
        layout.addWidget(self.title_label)

        if badge_widget:
            layout.addWidget(badge_widget)

        # Optional hint text (e.g., "Paste image: Ctrl+V")
        if hint_text:
            hint_label = QLabel(hint_text)
            hint_label.setStyleSheet(SECTION_HINT_STYLE)
            layout.addWidget(hint_label)

        # Info button (hidden by default, shown via set_info_tooltip)
        self.info_btn = None
        if show_info_button:
            self.info_btn = IconButton("info", size=16)
            info_effect = QGraphicsOpacityEffect(self.info_btn)
            info_effect.setOpacity(DISABLED_OPACITY)
            self.info_btn.setGraphicsEffect(info_effect)
            self.info_btn.setStyleSheet("""
                QPushButton {
                    background: transparent;
                    border: none;
                    padding: 2px;
                    min-width: 20px;
                    max-width: 20px;
                    min-height: 20px;
                    max-height: 20px;
                }
            """)
            self.info_btn.setCursor(Qt.ArrowCursor)
            self.info_btn.hide()
            layout.addWidget(self.info_btn)

        layout.addStretch()

        # Button container for tighter spacing
        btn_container = QWidget()
        btn_layout = QHBoxLayout(btn_container)
        btn_layout.setContentsMargins(0, 0, 0, 0)
        btn_layout.setSpacing(2)

        # Regenerate button (optional) - for re-running AI generation
        self.regenerate_btn = None
        if show_regenerate_button:
            self.regenerate_btn = IconButton("refresh-cw", size=16)
            self.regenerate_btn.setToolTip("Regenerate response")
            self.regenerate_btn.setStyleSheet(ICON_BTN_STYLE)
            self.regenerate_btn.clicked.connect(lambda: self.regenerate_requested.emit())
            btn_layout.addWidget(self.regenerate_btn)

        self.copy_content_btn = None
        if show_copy_content_button:
            self.copy_content_btn = IconButton("copy", size=16)
            self.copy_content_btn.setToolTip("Copy raw markdown")
            self.copy_content_btn.setStyleSheet(ICON_BTN_STYLE)
            self.copy_content_btn.clicked.connect(lambda: self.copy_content_requested.emit())
            btn_layout.addWidget(self.copy_content_btn)

        self.render_toggle_btn = None
        self._render_mode = True
        if show_render_toggle:
            self.render_toggle_btn = IconButton("edit", size=16)
            self.render_toggle_btn.setToolTip("Switch to raw edit mode")
            self.render_toggle_btn.setStyleSheet(ICON_BTN_STYLE)
            self.render_toggle_btn.clicked.connect(lambda: self.render_toggle_requested.emit())
            btn_layout.addWidget(self.render_toggle_btn)

        # Version navigation (optional) - shown only when multiple versions exist
        self.version_container = None
        self.version_prev_btn = None
        self.version_label = None
        self.version_next_btn = None
        if show_version_nav:
            self.version_container = QWidget()
            self.version_container.setStyleSheet("background: transparent;")
            version_layout = QHBoxLayout(self.version_container)
            version_layout.setContentsMargins(0, 0, 8, 0)
            version_layout.setSpacing(2)

            self.version_prev_btn = IconButton("chevron-left", size=16)
            self.version_prev_btn.setToolTip("Previous version")
            self.version_prev_btn.setStyleSheet(ICON_BTN_STYLE)
            self.version_prev_btn.clicked.connect(lambda: self.version_prev_requested.emit())
            self.version_prev_btn.setEnabled(False)
            version_layout.addWidget(self.version_prev_btn)

            self.version_label = QLabel("1 of 1")
            self.version_label.setStyleSheet(SECTION_HINT_STYLE)
            version_layout.addWidget(self.version_label)

            self.version_next_btn = IconButton("chevron-right", size=16)
            self.version_next_btn.setToolTip("Next version")
            self.version_next_btn.setStyleSheet(ICON_BTN_STYLE)
            self.version_next_btn.clicked.connect(lambda: self.version_next_requested.emit())
            self.version_next_btn.setEnabled(False)
            version_layout.addWidget(self.version_next_btn)

            self.version_container.hide()
            btn_layout.addWidget(self.version_container)

        # Wrap button (optional) - placed before undo/redo
        self.wrap_btn = None
        self._wrapped = True  # Default state: wrapped (height limited)
        if show_wrap_button:
            self.wrap_btn = IconButton("chevrons-up-down", size=16)
            self.wrap_btn.setToolTip("Expand to fit content")
            self.wrap_btn.setStyleSheet(ICON_BTN_STYLE)
            self.wrap_btn.clicked.connect(lambda: self.wrap_requested.emit())
            btn_layout.addWidget(self.wrap_btn)

        # Undo/redo buttons (optional)
        self.undo_btn = None
        self.redo_btn = None
        if show_undo_redo:
            self.undo_btn = IconButton("undo", size=16)
            self.undo_btn.setToolTip("Undo (Ctrl+Z)")
            self.undo_btn.setStyleSheet(ICON_BTN_STYLE)
            self.undo_btn.clicked.connect(lambda: self.undo_requested.emit())
            self.undo_btn.setEnabled(False)
            btn_layout.addWidget(self.undo_btn)

            self.redo_btn = IconButton("redo", size=16)
            self.redo_btn.setToolTip("Redo (Ctrl+Shift+Z)")
            self.redo_btn.setStyleSheet(ICON_BTN_STYLE)
            self.redo_btn.clicked.connect(lambda: self.redo_requested.emit())
            self.redo_btn.setEnabled(False)
            btn_layout.addWidget(self.redo_btn)

        # Delete button (optional) - for removing sections
        self.delete_btn = None
        if show_delete_button:
            self.delete_btn = IconButton("delete", size=16)
            self.delete_btn.setToolTip("Delete this section")
            self.delete_btn.setStyleSheet(ICON_BTN_STYLE)
            self.delete_btn.clicked.connect(lambda: self.delete_requested.emit())
            btn_layout.addWidget(self.delete_btn)

        # Save button (optional) - icon only, no border, on right side
        self.save_btn = None
        if show_save_button:
            self.save_btn = IconButton("save", size=16)
            self.save_btn.setToolTip(f"Save {title.lower()}")
            self.save_btn.setStyleSheet(ICON_BTN_STYLE)
            self.save_btn.clicked.connect(lambda: self.save_requested.emit())
            btn_layout.addWidget(self.save_btn)

        layout.addWidget(btn_container)

    def set_collapsed(self, collapsed: bool):
        """Update the visual state of the toggle button."""
        self._collapsed = collapsed
        icon_name = "chevron-right" if collapsed else "chevron-down"
        self.toggle_btn._icon_name = icon_name
        self.toggle_btn._update_icon(ICON_COLOR_NORMAL)
        self.toggle_btn.setToolTip("Expand section" if collapsed else "Collapse section")
        self._update_title_style()

    def set_has_content(self, has_content: bool):
        """Highlight title when section has content (visible when collapsed)."""
        self._has_content = has_content
        self._update_title_style()

    def set_info_tooltip(self, text: str):
        """Show the info button with the given tooltip text."""
        if self.info_btn and text:
            wrapped_text = f'<div style="max-width: 800px;">{text}</div>'
            self.info_btn.setToolTip(wrapped_text)
            self.info_btn.show()

    def _update_title_style(self):
        """Update title style based on collapsed + has_content state."""
        from modules.gui.shared.theme import SECTION_TITLE_ACTIVE_STYLE, SECTION_TITLE_STYLE

        if self._collapsed and self._has_content:
            self.title_label.setStyleSheet(SECTION_TITLE_ACTIVE_STYLE)
        else:
            self.title_label.setStyleSheet(SECTION_TITLE_STYLE)

    def set_title(self, title: str):
        """Update the section title."""
        self._title = title
        self.title_label.setText(title)

    def set_all_buttons_enabled(self, enabled: bool):
        for btn in (self.save_btn, self.undo_btn, self.redo_btn, self.wrap_btn):
            if btn:
                btn.setEnabled(enabled)

    def set_undo_redo_enabled(self, can_undo: bool, can_redo: bool):
        if self.undo_btn:
            self.undo_btn.setEnabled(can_undo)
        if self.redo_btn:
            self.redo_btn.setEnabled(can_redo)

    def set_delete_button_visible(self, visible: bool):
        """Show or hide the delete button."""
        if self.delete_btn:
            self.delete_btn.setVisible(visible)

    def set_wrap_state(self, wrapped: bool):
        """Update the wrap button icon based on state.

        Args:
            wrapped: True if content is wrapped (height limited), False if expanded
        """
        self._wrapped = wrapped
        if self.wrap_btn:
            # chevrons-up-down = wrapped (arrows pointing outward = can expand)
            # chevrons-down-up = unwrapped (arrows pointing inward = can compress)
            icon_name = "chevrons-up-down" if wrapped else "chevrons-down-up"
            self.wrap_btn._icon_name = icon_name
            self.wrap_btn._update_icon(ICON_COLOR_NORMAL)
            self.wrap_btn.setToolTip("Expand to fit content" if wrapped else "Wrap to fixed height")

    def set_wrap_button_visible(self, visible: bool):
        """Show or hide the wrap button."""
        if self.wrap_btn:
            self.wrap_btn.setVisible(visible)

    def is_wrapped(self) -> bool:
        """Return current wrap state."""
        return self._wrapped

    def is_collapsed(self) -> bool:
        """Return current collapsed state."""
        return self._collapsed

    def set_version_info(self, current: int, total: int):
        """Update version display. Shows only when total > 1.

        Args:
            current: Current version number (1-indexed)
            total: Total number of versions
        """
        if not self.version_container:
            return

        if total <= 1:
            self.version_container.hide()
            return

        self.version_container.show()
        self.version_label.setText(f"{current} of {total}")
        self.version_prev_btn.setEnabled(current > 1)
        self.version_next_btn.setEnabled(current < total)

    def set_regenerate_button_visible(self, visible: bool):
        """Show or hide the regenerate button."""
        if self.regenerate_btn:
            self.regenerate_btn.setVisible(visible)

    def set_regenerate_button_enabled(self, enabled: bool):
        """Enable or disable the regenerate button."""
        if self.regenerate_btn:
            self.regenerate_btn.setEnabled(enabled)

    def set_render_mode(self, rendered: bool):
        self._render_mode = rendered
        if self.render_toggle_btn:
            if rendered:
                self.render_toggle_btn._icon_name = "edit"
                self.render_toggle_btn._update_icon(ICON_COLOR_NORMAL)
                self.render_toggle_btn.setToolTip("Switch to raw edit mode")
            else:
                self.render_toggle_btn._icon_name = "eye"
                self.render_toggle_btn._update_icon(ICON_COLOR_NORMAL)
                self.render_toggle_btn.setToolTip("Switch to rendered view")


class ImageChipWidget(QWidget):
    """Chip widget for displaying an image in the editor."""

    delete_requested = Signal(int)
    copy_requested = Signal(int)

    # Styles matching ContextChipBase in context_widgets.py
    _chip_style = f"""
        QWidget#editorChip {{
            background-color: {COLOR_BUTTON_BG};
            border: 1px solid {COLOR_BORDER};
            border-radius: 12px;
            padding: 2px;
        }}
        QWidget#editorChip QPushButton {{
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 20px;
            max-width: 20px;
            min-height: 20px;
            max-height: 20px;
        }}
        QWidget#editorChip QLabel {{
            color: {COLOR_TEXT};
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }}
    """

    _chip_hover_style = f"""
        QWidget#editorChip {{
            background-color: {COLOR_BUTTON_HOVER};
            border: 1px solid {COLOR_BORDER};
            border-radius: 12px;
            padding: 2px;
        }}
        QWidget#editorChip QPushButton {{
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 20px;
            max-width: 20px;
            min-height: 20px;
            max-height: 20px;
        }}
        QWidget#editorChip QLabel {{
            color: {COLOR_TEXT};
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }}
    """

    def __init__(
        self,
        index: int,
        image_number: int,
        image_data: str,
        media_type: str,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.index = index
        self.image_data = image_data
        self.media_type = media_type

        self.setObjectName("editorChip")
        self.setAttribute(Qt.WA_StyledBackground, True)
        self.setStyleSheet(self._chip_style)
        self.setCursor(Qt.PointingHandCursor)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(6, 2, 4, 2)
        layout.setSpacing(4)

        # Copy button
        self.copy_btn = IconButton("copy", size=16)
        self.copy_btn.setCursor(Qt.PointingHandCursor)
        self.copy_btn.setToolTip("Copy to clipboard")
        self.copy_btn.clicked.connect(self._on_copy_clicked)
        layout.addWidget(self.copy_btn)

        # Label
        self.label = QLabel(f"[image #{image_number}]")
        layout.addWidget(self.label)

        # Delete button
        self.delete_btn = IconButton("delete", size=16)
        self.delete_btn.setCursor(Qt.PointingHandCursor)
        self.delete_btn.setToolTip("Remove image")
        self.delete_btn.clicked.connect(self._on_delete_clicked)
        layout.addWidget(self.delete_btn)

        # Setup tooltip with thumbnail
        self._setup_image_tooltip()

    def _setup_image_tooltip(self):
        """Set up tooltip with image thumbnail."""
        try:
            image_bytes = base64.b64decode(self.image_data)
            image = QImage()
            image.loadFromData(QByteArray(image_bytes))

            if image.isNull():
                self.setToolTip("Image preview unavailable")
                return

            orig_width = image.width()
            orig_height = image.height()

            thumbnail = image.scaled(300, 300, Qt.KeepAspectRatio, Qt.SmoothTransformation)

            buffer = QBuffer()
            buffer.open(QBuffer.WriteOnly)
            thumbnail.save(buffer, "PNG")
            thumb_base64 = base64.b64encode(buffer.data()).decode("utf-8")
            buffer.close()

            format_name = self.media_type.split("/")[-1].upper()
            tooltip_html = f"""
                <div style="text-align: center;">
                    <img src="data:image/png;base64,{thumb_base64}" /><br/>
                    <span style="color: #888888; font-size: 11px;">
                        {orig_width} x {orig_height} ({format_name})
                    </span>
                </div>
            """
            self.setToolTip(tooltip_html)
        except Exception as e:
            logger.warning(f"Failed to create image tooltip: {e}")
            self.setToolTip("Image preview unavailable")

    def _on_copy_clicked(self):
        self.copy_requested.emit(self.index)

    def _on_delete_clicked(self):
        self.delete_requested.emit(self.index)

    def copy_to_clipboard(self):
        """Copy image to clipboard."""
        try:
            image_bytes = base64.b64decode(self.image_data)
            image = QImage()
            image.loadFromData(QByteArray(image_bytes))
            if not image.isNull():
                QApplication.clipboard().setImage(image)
        except Exception as e:
            logger.warning(f"Failed to copy image to clipboard: {e}")

    def enterEvent(self, event):
        self.setStyleSheet(self._chip_hover_style)
        super().enterEvent(event)

    def leaveEvent(self, event):
        self.setStyleSheet(self._chip_style)
        super().leaveEvent(event)

    def mousePressEvent(self, event):
        # Copy on click (except on buttons)
        if not self.delete_btn.geometry().contains(event.pos()) and not self.copy_btn.geometry().contains(event.pos()):
            self._on_copy_clicked()
        super().mousePressEvent(event)


def create_text_edit(
    placeholder: str = "",
    min_height: int = TEXT_EDIT_MIN_HEIGHT,
    font_size: int = 12,
    is_bubble: bool = False,
) -> QTextEdit:
    """Create a pre-configured QTextEdit with standard styling.

    Args:
        placeholder: Placeholder text
        min_height: Minimum height in pixels (default: TEXT_EDIT_MIN_HEIGHT)
        font_size: Font size for monospace font
        is_bubble: If True, apply gentler bubble styling for conversation inputs

    Returns:
        Configured QTextEdit widget
    """
    text_edit = QTextEdit()
    text_edit.setFont(QFont("Menlo, Monaco, Consolas, monospace", font_size))
    text_edit.setLineWrapMode(QTextEdit.WidgetWidth)
    text_edit.setAcceptRichText(False)
    if min_height > 0:
        text_edit.setMinimumHeight(min_height)
    if placeholder:
        text_edit.setPlaceholderText(placeholder)

    if is_bubble:
        from modules.gui.shared.theme import (
            COLOR_BUBBLE_BORDER,
            COLOR_BUBBLE_TEXT_EDIT_BG,
            COLOR_SELECTION,
            COLOR_TEXT,
        )

        text_edit.setStyleSheet(f"""
            QTextEdit {{
                background-color: {COLOR_BUBBLE_TEXT_EDIT_BG};
                color: {COLOR_TEXT};
                border: none;
                border-top: 1px solid {COLOR_BUBBLE_BORDER};
                border-bottom: 1px solid {COLOR_BUBBLE_BORDER};
                border-radius: 0px;
                padding: 8px;
                margin-right: 14px;
                selection-background-color: {COLOR_SELECTION};
            }}
        """)

    return text_edit


def create_markdown_browser(min_height: int = BUBBLE_TEXT_EDIT_MIN_HEIGHT):
    from PySide6.QtWidgets import QTextBrowser

    from modules.gui.shared.theme import (
        COLOR_BUBBLE_BORDER,
        COLOR_BUBBLE_TEXT_EDIT_BG,
        COLOR_SELECTION,
        COLOR_TEXT,
    )

    browser = QTextBrowser()
    browser.setFont(QFont("sans-serif", 15))
    browser.setOpenLinks(False)
    browser.setOpenExternalLinks(False)
    browser.setReadOnly(True)
    browser.setLineWrapMode(QTextEdit.WidgetWidth)
    if min_height > 0:
        browser.setMinimumHeight(min_height)
    browser.setStyleSheet(f"""
        QTextBrowser {{
            background-color: {COLOR_BUBBLE_TEXT_EDIT_BG};
            color: {COLOR_TEXT};
            border: none;
            border-top: 1px solid {COLOR_BUBBLE_BORDER};
            border-bottom: 1px solid {COLOR_BUBBLE_BORDER};
            border-radius: 0px;
            padding: 8px;
            margin-right: 14px;
            selection-background-color: {COLOR_SELECTION};
        }}
    """)
    return browser


def create_header_button(
    icon_name: str,
    tooltip: str,
    on_click: Callable[[], None],
    enabled: bool = True,
) -> IconButton:
    """Create a styled header/toolbar button with consistent appearance.

    Args:
        icon_name: Name of the icon to display
        tooltip: Tooltip text for the button
        on_click: Callback function when button is clicked
        enabled: Whether button is initially enabled (default: True)

    Returns:
        Configured IconButton widget
    """
    btn = IconButton(icon_name, size=HEADER_ICON_SIZE)
    btn.setToolTip(tooltip)
    btn.setStyleSheet(ICON_BTN_STYLE)
    btn.clicked.connect(on_click)
    btn.setEnabled(enabled)
    return btn


class ExpandableTextSection(QWidget):
    """A collapsible text section with wrap/expand functionality and undo/redo support.

    This widget combines a CollapsibleSectionHeader with a QTextEdit and provides:
    - Collapse/expand toggle for the section
    - Wrap/expand toggle for the text area (300px default vs fit content)
    - Built-in undo/redo state management
    - Optional save button

    Signals:
        collapsed_changed(bool): Emitted when section is collapsed/expanded
        wrapped_changed(bool): Emitted when wrap state changes
        text_changed(): Emitted when text content changes
        save_requested(): Emitted when save button is clicked
    """

    collapsed_changed = Signal(bool)
    wrapped_changed = Signal(bool)
    text_changed = Signal()
    save_requested = Signal()

    # Default height when wrapped
    DEFAULT_WRAPPED_HEIGHT = 300

    def __init__(
        self,
        title: str,
        show_save_button: bool = False,
        show_undo_redo: bool = True,
        show_wrap_button: bool = True,
        placeholder: str = "",
        hint_text: str = "",
        parent: QWidget | None = None,
    ):
        """Create an expandable text section.

        Args:
            title: Section header title
            show_save_button: Show save button in header
            show_undo_redo: Show undo/redo buttons in header
            show_wrap_button: Show wrap/expand toggle button
            placeholder: Placeholder text for the text edit
            hint_text: Optional hint text in header
            parent: Parent widget
        """
        super().__init__(parent)
        self._collapsed = False
        self._wrapped = True  # Default: wrapped (height limited)
        self._title = title
        self._undo_helper: TextEditUndoHelper | None = None

        self._setup_ui(
            title,
            show_save_button,
            show_undo_redo,
            show_wrap_button,
            placeholder,
            hint_text,
        )

    def _setup_ui(
        self,
        title: str,
        show_save_button: bool,
        show_undo_redo: bool,
        show_wrap_button: bool,
        placeholder: str,
        hint_text: str,
    ):
        """Set up the section UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)

        # Header
        self.header = CollapsibleSectionHeader(
            title,
            show_save_button=show_save_button,
            show_undo_redo=show_undo_redo,
            show_wrap_button=show_wrap_button,
            hint_text=hint_text,
        )
        self.header.toggle_requested.connect(self._on_toggle)
        self.header.wrap_requested.connect(self._on_wrap_toggle)
        self.header.undo_requested.connect(self.undo)
        self.header.redo_requested.connect(self.redo)
        self.header.save_requested.connect(lambda: self.save_requested.emit())
        layout.addWidget(self.header)

        # Text edit
        self.text_edit = create_text_edit(placeholder=placeholder, min_height=0)
        layout.addWidget(self.text_edit)

        # Initialize undo helper
        self._undo_helper = TextEditUndoHelper(
            self.text_edit,
            self._update_undo_redo_buttons,
        )
        self._undo_helper.initialize()
        self.text_edit.textChanged.connect(self._on_text_changed)

        # Apply initial wrap state
        self._apply_wrap_state()

    def _on_toggle(self):
        """Handle collapse/expand toggle."""
        self._collapsed = not self._collapsed
        self.text_edit.setVisible(not self._collapsed)
        self.header.set_collapsed(self._collapsed)
        self.collapsed_changed.emit(self._collapsed)

    def _on_wrap_toggle(self):
        """Handle wrap/expand toggle."""
        self._wrapped = not self._wrapped
        self.header.set_wrap_state(self._wrapped)
        self._apply_wrap_state()
        self.wrapped_changed.emit(self._wrapped)

    def _apply_wrap_state(self):
        """Apply the current wrap state to the text edit."""
        if self._wrapped:
            self.text_edit.setMaximumHeight(self.DEFAULT_WRAPPED_HEIGHT)
            self.text_edit.setMinimumHeight(0)
        else:
            self.text_edit.setMaximumHeight(16777215)  # QWIDGETSIZE_MAX
            self.text_edit.setMinimumHeight(0)

    def _on_text_changed(self):
        """Handle text changes - debounce state saving."""
        self._undo_helper.schedule_save()
        self.text_changed.emit()

    def _update_undo_redo_buttons(self, can_undo: bool, can_redo: bool):
        """Update undo/redo button enabled states."""
        self.header.set_undo_redo_enabled(can_undo, can_redo)

    # Public API

    def undo(self):
        """Undo last text change."""
        self._undo_helper.undo()

    def redo(self):
        """Redo last undone text change."""
        self._undo_helper.redo()

    def set_text(self, text: str):
        """Set the text content without affecting undo stack."""
        self.text_edit.blockSignals(True)
        self.text_edit.setPlainText(text)
        self.text_edit.blockSignals(False)
        self._undo_helper.initialize(text)

    def get_text(self) -> str:
        """Get the current text content."""
        return self.text_edit.toPlainText()

    def set_collapsed(self, collapsed: bool):
        """Set the collapsed state."""
        if self._collapsed != collapsed:
            self._collapsed = collapsed
            self.text_edit.setVisible(not collapsed)
            self.header.set_collapsed(collapsed)

    def is_collapsed(self) -> bool:
        """Return whether the section is collapsed."""
        return self._collapsed

    def set_wrapped(self, wrapped: bool):
        """Set the wrapped state."""
        if self._wrapped != wrapped:
            self._wrapped = wrapped
            self.header.set_wrap_state(wrapped)
            self._apply_wrap_state()

    def is_wrapped(self) -> bool:
        """Return whether the text is wrapped (height limited)."""
        return self._wrapped

    def clear_undo_stack(self):
        """Clear the undo/redo stacks."""
        self._undo_helper.clear()


class UndoRedoManager(Generic[T]):
    """Generic undo/redo state manager with debounced state saving.

    This class provides a reusable undo/redo implementation that can work with
    any state type. It includes automatic debouncing of state saves to avoid
    creating too many undo points during rapid changes.

    Usage:
        # Define state getter and restorer
        def get_state() -> MyState:
            return MyState(text=self.text_edit.toPlainText())

        def restore_state(state: MyState):
            self.text_edit.setPlainText(state.text)

        def on_stack_changed(can_undo: bool, can_redo: bool):
            self.undo_btn.setEnabled(can_undo)
            self.redo_btn.setEnabled(can_redo)

        # Create manager
        self._undo_manager = UndoRedoManager(
            get_state=get_state,
            restore_state=restore_state,
            on_stack_changed=on_stack_changed,
        )

        # Connect to text changes
        self.text_edit.textChanged.connect(self._undo_manager.schedule_save)

        # Connect undo/redo buttons
        self.undo_btn.clicked.connect(self._undo_manager.undo)
        self.redo_btn.clicked.connect(self._undo_manager.redo)
    """

    def __init__(
        self,
        get_state: Callable[[], T],
        restore_state: Callable[[T], None],
        on_stack_changed: Callable[[bool, bool], None],
        debounce_ms: int = 500,
    ):
        """Create an undo/redo manager.

        Args:
            get_state: Function that returns the current state
            restore_state: Function that restores a previous state
            on_stack_changed: Callback when undo/redo availability changes.
                              Called with (can_undo, can_redo) booleans.
            debounce_ms: Milliseconds to wait before saving state (default: 500)
        """
        self._get_state = get_state
        self._restore_state = restore_state
        self._on_stack_changed = on_stack_changed

        self._undo_stack: list[T] = []
        self._redo_stack: list[T] = []
        self._last_state: T | None = None

        self._timer = QTimer()
        self._timer.setSingleShot(True)
        self._timer.setInterval(debounce_ms)
        self._timer.timeout.connect(self._save_if_changed)

    def initialize(self):
        """Initialize with current state. Call after UI setup."""
        self._last_state = self._get_state()

    def schedule_save(self):
        """Schedule a debounced state save. Call on every change."""
        self._timer.start()

    def save_now(self):
        """Save current state immediately if changed."""
        if self._last_state is None:
            self._last_state = self._get_state()
            return

        current = self._get_state()
        if current != self._last_state:
            self._undo_stack.append(self._last_state)
            self._redo_stack.clear()
            self._last_state = current
            self._notify_changed()

    def _save_if_changed(self):
        """Internal: Save state if changed (called by timer)."""
        self.save_now()

    def undo(self) -> bool:
        """Undo last change.

        Returns:
            True if undo was performed, False if nothing to undo
        """
        if not self._undo_stack:
            return False
        self._redo_stack.append(self._get_state())
        state = self._undo_stack.pop()
        self._restore_state(state)
        self._last_state = state
        self._notify_changed()
        return True

    def redo(self) -> bool:
        """Redo last undone change.

        Returns:
            True if redo was performed, False if nothing to redo
        """
        if not self._redo_stack:
            return False
        self._undo_stack.append(self._get_state())
        state = self._redo_stack.pop()
        self._restore_state(state)
        self._last_state = state
        self._notify_changed()
        return True

    def clear(self):
        """Clear all undo/redo history and reinitialize."""
        self._undo_stack.clear()
        self._redo_stack.clear()
        self._last_state = self._get_state()
        self._notify_changed()

    def _notify_changed(self):
        """Notify listener of stack state change."""
        self._on_stack_changed(bool(self._undo_stack), bool(self._redo_stack))

    @property
    def can_undo(self) -> bool:
        """Whether there are changes to undo."""
        return bool(self._undo_stack)

    @property
    def can_redo(self) -> bool:
        """Whether there are changes to redo."""
        return bool(self._redo_stack)


class ImageChipContainer(QWidget):
    """Container widget for managing a list of image chips.

    This widget handles displaying, adding, and removing images as chips.
    It provides a reusable component for dialogs that need to manage
    multiple images with paste-from-clipboard support.

    Signals:
        images_changed: Emitted when images are added or removed

    Usage:
        self.images_container = ImageChipContainer(
            clipboard_manager=self.clipboard_manager,
            notification_manager=self.notification_manager,
        )
        layout.addWidget(self.images_container)

        # Set initial images
        self.images_container.set_images(image_list)

        # Paste from clipboard (e.g., in Ctrl+V handler)
        if self.images_container.paste_from_clipboard():
            return  # Image was pasted
    """

    images_changed = Signal()

    def __init__(
        self,
        clipboard_manager=None,
        notification_manager=None,
        parent: QWidget | None = None,
    ):
        """Create an image chip container.

        Args:
            clipboard_manager: Manager for clipboard operations (optional)
            notification_manager: Manager for notifications (optional)
            parent: Parent widget
        """
        super().__init__(parent)
        self._clipboard_manager = clipboard_manager
        self._notification_manager = notification_manager
        self._images: list[ContextItem] = []
        self._chips: list[ImageChipWidget] = []

        self.setStyleSheet("background: transparent;")
        self._layout = QHBoxLayout(self)
        self._layout.setContentsMargins(0, 0, 0, 4)
        self._layout.setSpacing(6)
        self._layout.addStretch()
        self.hide()

    @property
    def images(self) -> list["ContextItem"]:
        """Get a copy of the current images list."""
        return list(self._images)

    def set_images(self, images: list["ContextItem"]):
        """Set images and rebuild chips.

        Args:
            images: List of ContextItem objects with image data
        """
        self._images = list(images)
        self._rebuild_chips()

    def add_image(self, image: "ContextItem"):
        """Add a single image to the container.

        Args:
            image: ContextItem with image data
        """
        self._images.append(image)
        self._rebuild_chips()

    def clear(self):
        """Remove all images from the container."""
        self._images.clear()
        self._rebuild_chips()

    def paste_from_clipboard(self) -> bool:
        """Paste image from clipboard if available.

        Returns:
            True if an image was pasted, False otherwise
        """
        if not self._clipboard_manager or not self._clipboard_manager.has_image():
            return False

        image_data = self._clipboard_manager.get_image_data()
        if image_data:
            base64_data, media_type = image_data
            # Import here to avoid circular imports
            from core.context_manager import ContextItem, ContextItemType

            self.add_image(
                ContextItem(
                    item_type=ContextItemType.IMAGE,
                    data=base64_data,
                    media_type=media_type,
                )
            )
            return True
        return False

    def _rebuild_chips(self):
        """Rebuild all image chips from current state."""
        # Clear existing chips
        for chip in self._chips:
            chip.deleteLater()
        self._chips.clear()

        # Remove all items from layout
        while self._layout.count():
            self._layout.takeAt(0)

        if not self._images:
            self.hide()
            self.images_changed.emit()
            return

        self.show()
        for idx, item in enumerate(self._images):
            chip = ImageChipWidget(
                index=idx,
                image_number=idx + 1,
                image_data=item.data or "",
                media_type=item.media_type or "image/png",
            )
            chip.delete_requested.connect(self._on_delete)
            chip.copy_requested.connect(self._on_copy)
            self._chips.append(chip)
            self._layout.addWidget(chip)

        self._layout.addStretch()
        self.images_changed.emit()

    def _on_delete(self, index: int):
        """Handle image deletion request."""
        if 0 <= index < len(self._images):
            del self._images[index]
            self._rebuild_chips()

    def _on_copy(self, index: int):
        """Handle image copy request."""
        if 0 <= index < len(self._chips):
            self._chips[index].copy_to_clipboard()
            if self._notification_manager and is_notification_enabled("clipboard_copy"):
                self._notification_manager.show_success_notification("Copied")
