"""Centralized GUI theme - single source of truth for all colors and stylesheets."""

import os
from collections.abc import Callable
from typing import TypeVar

from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QDialog, QSizePolicy, QWidget

# Timing constants
DIALOG_SHOW_DELAY_MS = 25
TEXT_CHANGE_DEBOUNCE_MS = 500

# Layout constants
DEFAULT_WRAPPED_HEIGHT = 300
QWIDGETSIZE_MAX = 16777215

# Scroll area margin constants
DIALOG_CONTENT_MARGINS = (10, 10, 0, 10)  # No right margin - scrollbar at edge
SCROLL_CONTENT_MARGINS = (0, 0, 0, 0)  # Individual widgets handle their margins
SCROLL_CONTENT_SPACING = 8

# Button layout constants
BUTTON_ROW_SPACING = 8

# Header/toolbar layout constants
HEADER_RIGHT_MARGIN = 12
HEADER_ICON_SIZE = 16
HEADER_BUTTON_SPACING = 2

# Window size defaults
DEFAULT_DIALOG_SIZE = (600, 500)
MIN_DIALOG_SIZE = (500, 400)
SMALL_DIALOG_SIZE = (600, 400)
SMALL_MIN_DIALOG_SIZE = (400, 300)

# =============================================================================
# Colors - Single source of truth for all GUI components
# =============================================================================

# Background colors
COLOR_DIALOG_BG = "#2b2b2b"
COLOR_TEXT_EDIT_BG = "#1e1e1e"
COLOR_BUBBLE_TEXT_EDIT_BG = "#1a1a1a"  # Slightly darker than main text edit
COLOR_BUBBLE_BORDER = "#444444"  # Subtler border for bubbles

# Text colors
COLOR_TEXT = "#f0f0f0"
COLOR_TEXT_WHITE = "#ffffff"
COLOR_TEXT_SECONDARY = "#888888"
COLOR_TEXT_HINT = "#666666"
COLOR_TEXT_LIGHT_GRAY = "#cccccc"
COLOR_TEXT_LIGHT = "#aaaaaa"

# Notification text (light theme)
COLOR_NOTIFICATION_TITLE = "#1a1a1a"
COLOR_NOTIFICATION_BODY = "#4a4a4a"

# Border colors
COLOR_BORDER = "#555555"

# Button colors
COLOR_BUTTON_BG = "#3a3a3a"
COLOR_BUTTON_HOVER = "#454545"
COLOR_BUTTON_PRESSED = "#505050"
COLOR_BUTTON_DISABLED_BG = "#2a2a2a"
COLOR_BUTTON_DISABLED_TEXT = "#444444"

# Selection color
COLOR_SELECTION = "#3d6a99"

# Scrollbar colors
COLOR_SCROLLBAR_HANDLE = "#555555"
COLOR_SCROLLBAR_HANDLE_HOVER = "#666666"

# Tooltip colors
COLOR_TOOLTIP_BG = "#0d0d0d"
COLOR_TOOLTIP_BORDER = "#444444"

# ComboBox colors
COLOR_COMBOBOX_ARROW = "#cccccc"

# Syntax highlighting colors
COLOR_PLACEHOLDER = "#d19a66"

# Error state colors
COLOR_ERROR_BG = "#3a2a2a"
COLOR_ERROR_BG_HOVER = "#453535"
COLOR_ERROR_BORDER = "#775555"

# Conversation turn accent colors
COLOR_ACCENT_USER = "#4a9eff"
COLOR_ACCENT_USER_DIM = "rgba(74, 158, 255, 0.12)"
COLOR_ACCENT_ASSISTANT = "#a855f7"
COLOR_ACCENT_ASSISTANT_DIM = "rgba(168, 85, 247, 0.12)"

# =============================================================================
# SVG Paths
# =============================================================================

_SVG_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "icons", "svg")
_SVG_CHEVRON_DOWN_PATH = os.path.join(_SVG_DIR, "chevron-down.svg")
_SVG_CHEVRON_UP_PATH = os.path.join(_SVG_DIR, "chevron-up.svg")

SVG_CHEVRON_DOWN_PATH = _SVG_CHEVRON_DOWN_PATH
SVG_CHEVRON_UP_PATH = _SVG_CHEVRON_UP_PATH

# =============================================================================
# Stylesheets - Single source of truth for all widget styles
# =============================================================================

# Dark theme tooltip style - single source of truth for all tooltips
TOOLTIP_STYLE = f"""
    QToolTip {{
        background-color: {COLOR_TOOLTIP_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_TOOLTIP_BORDER};
        border-radius: 2px;
        padding: 2px 2px;
    }}
"""

COMBOBOX_STYLE = f"""
    QComboBox {{
        background-color: {COLOR_BUTTON_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 4px 24px 4px 8px;
        min-width: 50px;
    }}
    QComboBox:hover {{
        background-color: {COLOR_BUTTON_HOVER};
    }}
    QComboBox::drop-down {{
        subcontrol-origin: padding;
        subcontrol-position: top right;
        width: 20px;
        border: none;
    }}
    QComboBox::down-arrow {{
        image: url("{_SVG_CHEVRON_DOWN_PATH}");
        width: 12px;
        height: 12px;
    }}
    QComboBox QAbstractItemView {{
        background-color: {COLOR_DIALOG_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        selection-background-color: {COLOR_SELECTION};
        outline: none;
    }}
"""

SPINBOX_STYLE = f"""
    QSpinBox {{
        background-color: {COLOR_BUTTON_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 4px 8px;
        min-width: 60px;
    }}
    QSpinBox::up-button, QSpinBox::down-button {{
        background-color: transparent;
        border: none;
        width: 16px;
    }}
    QSpinBox::up-arrow {{
        image: url("{_SVG_CHEVRON_UP_PATH}");
        width: 10px;
        height: 10px;
    }}
    QSpinBox::down-arrow {{
        image: url("{_SVG_CHEVRON_DOWN_PATH}");
        width: 10px;
        height: 10px;
    }}
"""

# Complete dialog stylesheet
DIALOG_STYLESHEET = f"""
    QDialog {{
        background-color: {COLOR_DIALOG_BG};
        color: {COLOR_TEXT};
    }}
    QTextEdit {{
        background-color: {COLOR_TEXT_EDIT_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 8px;
        margin-right: 14px;
        selection-background-color: {COLOR_SELECTION};
    }}
    QPushButton {{
        background-color: {COLOR_BUTTON_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 6px 12px;
    }}
    QPushButton:hover {{
        background-color: {COLOR_BUTTON_HOVER};
    }}
    QPushButton:pressed {{
        background-color: {COLOR_BUTTON_PRESSED};
    }}
    QPushButton:disabled {{
        background-color: {COLOR_BUTTON_DISABLED_BG};
        color: {COLOR_BUTTON_DISABLED_TEXT};
    }}
    QScrollBar:vertical {{
        background-color: {COLOR_DIALOG_BG};
        width: 12px;
        border-radius: 6px;
    }}
    QScrollBar::handle:vertical {{
        background-color: {COLOR_SCROLLBAR_HANDLE};
        border-radius: 6px;
        min-height: 20px;
    }}
    QScrollBar::handle:vertical:hover {{
        background-color: {COLOR_SCROLLBAR_HANDLE_HOVER};
    }}
    QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
        height: 0;
    }}
    QScrollBar:horizontal {{
        background-color: {COLOR_DIALOG_BG};
        height: 12px;
        border-radius: 6px;
    }}
    QScrollBar::handle:horizontal {{
        background-color: {COLOR_SCROLLBAR_HANDLE};
        border-radius: 6px;
        min-width: 20px;
    }}
    QScrollBar::handle:horizontal:hover {{
        background-color: {COLOR_SCROLLBAR_HANDLE_HOVER};
    }}
    QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{
        width: 0;
    }}
    QScrollArea {{
        background: transparent;
        border: none;
    }}
    QScrollArea > QWidget > QWidget {{
        background-color: {COLOR_DIALOG_BG};
    }}
"""

# Menu stylesheet (for context menu and tray menu)
MENU_STYLESHEET = f"""
    QMenu {{
        background-color: {COLOR_DIALOG_BG};
        border: 1px solid {COLOR_BORDER};
        border-radius: 6px;
        padding: 4px;
        color: {COLOR_TEXT_WHITE};
        font-size: 13px;
    }}
    QMenu::item {{
        background-color: transparent;
        padding: 8px 16px;
        border-radius: 4px;
        margin: 1px;
    }}
    QMenu::item:selected, QMenu::item:focus {{
        background-color: {COLOR_BUTTON_HOVER};
        color: {COLOR_TEXT_WHITE};
    }}
    QMenu::item:disabled {{
        color: {COLOR_TEXT_HINT};
    }}
    QMenu::separator {{
        height: 1px;
        background-color: {COLOR_BORDER};
        margin: 4px 8px;
    }}
    QMenu::indicator {{
        width: 16px;
        height: 16px;
        margin-left: 4px;
    }}
"""

# Chip styles
CHIP_STYLE = f"""
    QWidget#chip {{
        background-color: {COLOR_BUTTON_BG};
        border: 1px solid {COLOR_BORDER};
        border-radius: 12px;
        padding: 2px;
    }}
"""

CHIP_HOVER_STYLE = f"""
    QWidget#chip {{
        background-color: {COLOR_BUTTON_HOVER};
        border: 1px solid {COLOR_BORDER};
        border-radius: 12px;
        padding: 2px;
    }}
"""

CHIP_DISABLED_STYLE = f"""
    QWidget#chip {{
        background-color: {COLOR_BUTTON_DISABLED_BG};
        border: 1px solid {COLOR_BUTTON_DISABLED_TEXT};
        border-radius: 12px;
        padding: 2px;
    }}
"""

# Label styles
CHIP_LABEL_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT};
        font-size: 12px;
        padding: 2px 4px;
        background: transparent;
    }}
"""

CHIP_LABEL_DISABLED_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT_HINT};
        font-size: 12px;
        padding: 2px 4px;
        background: transparent;
    }}
"""

SECTION_TITLE_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT_SECONDARY};
        font-size: 11px;
        font-weight: normal;
    }}
"""

SECTION_TITLE_ACTIVE_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT_LIGHT_GRAY};
        font-size: 11px;
        font-weight: bold;
    }}
"""

SECTION_HINT_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT_HINT};
        font-size: 11px;
    }}
"""

ROLE_BADGE_USER_STYLE = f"""
    QLabel {{
        color: {COLOR_ACCENT_USER};
        background-color: {COLOR_ACCENT_USER_DIM};
        font-size: 9px;
        font-weight: bold;
        padding: 1px 5px;
        border-radius: 4px;
    }}
"""

ROLE_BADGE_ASSISTANT_STYLE = f"""
    QLabel {{
        color: {COLOR_ACCENT_ASSISTANT};
        background-color: {COLOR_ACCENT_ASSISTANT_DIM};
        font-size: 9px;
        font-weight: bold;
        padding: 1px 5px;
        border-radius: 4px;
    }}
"""

TURN_NUMBER_STYLE = f"""
    QLabel {{
        color: {COLOR_TEXT_HINT};
        font-size: 11px;
        font-family: "Menlo", "Monaco", "Consolas", monospace;
    }}
"""

# Icon button style
ICON_BTN_STYLE = f"""
    QPushButton {{
        background: transparent;
        border: none;
        padding: 2px;
        min-width: 20px;
        max-width: 20px;
        min-height: 20px;
        max-height: 20px;
    }}
""" + TOOLTIP_STYLE

# Tab styles
TAB_STYLE = f"""
    QWidget {{
        background: transparent;
        border: none;
        border-bottom: 2px solid transparent;
        padding: 4px 8px 2px 8px;
    }}
    QWidget:hover {{
        background: rgba(255, 255, 255, 0.05);
    }}
    QWidget[active="true"] {{
        border-bottom: 2px solid {COLOR_TEXT_SECONDARY};
    }}
"""

TAB_LABEL_STYLE = f"border: none; background: transparent; color: {COLOR_TEXT_LIGHT_GRAY};"

TAB_CLOSE_BTN_STYLE = f"""
    QPushButton {{
        background: transparent;
        border: none;
        border-radius: 9px;
        padding: 0;
        margin: 0;
    }}
    QPushButton:hover {{
        background: {COLOR_BORDER};
    }}
"""

# Notification label styles (light theme)
NOTIFICATION_TITLE_STYLE = f"""
    QLabel {{
        color: {COLOR_NOTIFICATION_TITLE};
        font-size: 14px;
        font-weight: 600;
        background: transparent;
        border: none;
    }}
"""

NOTIFICATION_BODY_STYLE = f"""
    QLabel {{
        color: {COLOR_NOTIFICATION_BODY};
        font-size: 13px;
        background: transparent;
        border: none;
    }}
"""


# =============================================================================
# Utility Functions
# =============================================================================

def get_dialog_stylesheet() -> str:
    """Return complete dialog stylesheet with tooltip style."""
    return DIALOG_STYLESHEET + TOOLTIP_STYLE


def get_text_edit_content_height(text_edit, min_height: int = 100) -> int:
    """Calculate the height needed to display all content without scrolling.

    Args:
        text_edit: The QTextEdit widget
        min_height: Minimum height to return (default: 100)

    Returns:
        Height in pixels needed to display all content
    """
    doc = text_edit.document()
    doc.setTextWidth(text_edit.viewport().width())
    margins = text_edit.contentsMargins()
    height = int(doc.size().height()) + margins.top() + margins.bottom() + 6
    return max(height, min_height)


def apply_wrap_state(text_edit, is_wrapped: bool, wrapped_height: int = DEFAULT_WRAPPED_HEIGHT):
    """Apply wrap state to a text edit widget.

    This is the shared implementation for wrap/expand toggling across all dialogs.

    Behavior:
    - Wrapped: text edit fills available space, uses internal scrollbar
    - Expanded: text edit grows to fit content, may need external scroll area

    Args:
        text_edit: The QTextEdit widget to modify
        is_wrapped: True to wrap (fill space), False to expand (fit content)
        wrapped_height: Unused, kept for API compatibility
    """
    if is_wrapped:
        # Fill available space, use internal scrollbar
        text_edit.setMinimumHeight(0)
        text_edit.setMaximumHeight(QWIDGETSIZE_MAX)
    else:
        # Expand to fit content (may exceed dialog, needs scroll area)
        content_height = get_text_edit_content_height(text_edit)
        text_edit.setMinimumHeight(content_height)
        text_edit.setMaximumHeight(QWIDGETSIZE_MAX)


def apply_section_size_policy(container: QWidget, expanding: bool = False, widget: QWidget = None):
    """Apply size policy to a dialog section container.

    Args:
        container: The section container widget
        expanding: If True, section expands to fill space. If False, section takes minimum needed.
        widget: Optional inner widget (e.g., text edit) to also set expanding policy on
    """
    if expanding:
        container.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Expanding)
        if widget:
            widget.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
    else:
        container.setSizePolicy(QSizePolicy.Preferred, QSizePolicy.Maximum)


# Dialog singleton management
T = TypeVar("T", bound=QDialog)


def create_singleton_dialog_manager() -> Callable:
    """Create a singleton dialog manager for preventing duplicate windows.

    Returns a function that manages dialog creation and visibility.

    Usage:
        _show_dialog = create_singleton_dialog_manager()

        def show_my_dialog(...):
            _show_dialog("my_dialog", lambda: MyDialog(...))
    """
    _dialogs: dict[str, QDialog] = {}

    def show_dialog(
        key: str,
        create_fn: Callable[[], T],
        delay_ms: int = DIALOG_SHOW_DELAY_MS,
    ) -> T | None:
        """Show a singleton dialog, bringing existing one to front if open.

        Args:
            key: Unique identifier for this dialog type/instance
            create_fn: Factory function to create the dialog
            delay_ms: Delay before creating dialog (for menu cleanup)

        Returns:
            None (dialog is shown asynchronously)
        """
        if key in _dialogs:
            dialog = _dialogs[key]
            dialog.raise_()
            dialog.activateWindow()
            return None

        def create_and_show():
            if key in _dialogs:
                _dialogs[key].raise_()
                _dialogs[key].activateWindow()
                return

            dialog = create_fn()
            _dialogs[key] = dialog
            dialog.finished.connect(lambda: _dialogs.pop(key, None))
            dialog.show()
            dialog.raise_()
            dialog.activateWindow()

        QTimer.singleShot(delay_ms, create_and_show)
        return None

    return show_dialog
