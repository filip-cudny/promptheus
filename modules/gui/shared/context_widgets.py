"""Context section widgets for displaying and managing context items as chips."""

import base64
import contextlib
import logging
from collections.abc import Callable

from PySide6.QtCore import QBuffer, QByteArray, QPoint, QSize, Qt, Signal
from PySide6.QtGui import QImage
from PySide6.QtWidgets import (
    QApplication,
    QGraphicsOpacityEffect,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from core.context_manager import ContextItemType, ContextManager
from modules.gui.icons import (
    DISABLED_OPACITY,
    ICON_COLOR_DISABLED,
    ICON_COLOR_HOVER,
    ICON_COLOR_NORMAL,
    create_icon,
)
from modules.gui.shared.theme import TOOLTIP_STYLE


class IconButton(QPushButton):
    """QPushButton with SVG icon that changes color on hover."""

    def __init__(
        self,
        icon_name: str,
        size: int = 16,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._icon_name = icon_name
        self._icon_size = size
        self._update_icon(ICON_COLOR_NORMAL)
        self.setIconSize(QSize(size, size))

    def _update_icon(self, color: str):
        """Update the icon with specified color."""
        self.setIcon(create_icon(self._icon_name, color, self._icon_size))

    def enterEvent(self, event):
        """Change icon color on hover."""
        if self.isEnabled():
            self._update_icon(ICON_COLOR_HOVER)
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Restore icon color when not hovering."""
        if self.isEnabled():
            self._update_icon(ICON_COLOR_NORMAL)
        super().leaveEvent(event)

    def setEnabled(self, enabled: bool):
        """Update icon color and opacity when enabled state changes."""
        super().setEnabled(enabled)
        if enabled:
            self._update_icon(ICON_COLOR_NORMAL)
            self.setGraphicsEffect(None)  # Remove opacity effect
        else:
            self._update_icon(ICON_COLOR_DISABLED)
            # Apply opacity effect for clear visual feedback
            effect = QGraphicsOpacityEffect(self)
            effect.setOpacity(DISABLED_OPACITY)
            self.setGraphicsEffect(effect)

    def set_icon(self, icon_name: str):
        """Change the button's icon.

        Args:
            icon_name: Name of the new icon to display
        """
        self._icon_name = icon_name
        self._update_icon(ICON_COLOR_NORMAL)


logger = logging.getLogger(__name__)


class ContextChipBase(QWidget):
    """Base class for context chips with copy and delete buttons."""

    delete_requested = Signal(int)  # Emits item index
    copy_requested = Signal(int)  # Emits item index

    _chip_style = """
        QWidget#chip {
            background-color: #3a3a3a;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _chip_hover_style = """
        QWidget#chip {
            background-color: #454545;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _label_style = """
        QLabel {
            color: #f0f0f0;
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _icon_btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 20px;
            max-width: 20px;
            min-height: 20px;
            max-height: 20px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(self, index: int, parent: QWidget | None = None):
        super().__init__(parent)
        self.index = index
        self.setObjectName("chip")
        self.setAttribute(Qt.WA_StyledBackground, True)  # Enable background painting
        self.setStyleSheet(self._chip_style)
        self.setCursor(Qt.PointingHandCursor)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(6, 2, 4, 2)
        layout.setSpacing(4)

        # Copy icon at the beginning
        self.copy_btn = IconButton("copy", size=16)
        self.copy_btn.setStyleSheet(self._icon_btn_style)
        self.copy_btn.setCursor(Qt.PointingHandCursor)
        self.copy_btn.setToolTip("Copy to clipboard")
        self.copy_btn.clicked.connect(self._on_copy_clicked)
        layout.addWidget(self.copy_btn)

        self.label = QLabel()
        self.label.setStyleSheet(self._label_style)
        layout.addWidget(self.label)

        # Delete button (x) at the end
        self.delete_btn = IconButton("delete", size=16)
        self.delete_btn.setStyleSheet(self._icon_btn_style)
        self.delete_btn.setCursor(Qt.PointingHandCursor)
        self.delete_btn.setToolTip("Remove from context")
        self.delete_btn.clicked.connect(self._on_delete_clicked)
        layout.addWidget(self.delete_btn)

        self.setSizePolicy(QSizePolicy.Maximum, QSizePolicy.Fixed)

    def _on_delete_clicked(self):
        """Handle delete button click."""
        self.delete_requested.emit(self.index)

    def _on_copy_clicked(self):
        """Handle copy button click."""
        self.copy_requested.emit(self.index)

    def mousePressEvent(self, event):
        """Handle mouse press - copy on click (except on buttons)."""
        # Check if click is on the delete button area
        delete_btn_rect = self.delete_btn.geometry()
        copy_btn_rect = self.copy_btn.geometry()

        if not delete_btn_rect.contains(event.pos()) and not copy_btn_rect.contains(event.pos()):
            self._on_copy_clicked()
        super().mousePressEvent(event)

    def set_label_text(self, text: str):
        """Set the chip label text."""
        self.label.setText(text)

    def enterEvent(self, event):
        """Handle mouse enter - show hover state."""
        self.setStyleSheet(self._chip_hover_style)
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Handle mouse leave - restore normal state."""
        self.setStyleSheet(self._chip_style)
        super().leaveEvent(event)

    def copy_to_clipboard(self):
        """Copy chip content to clipboard. Override in subclasses."""
        pass


class TextContextChip(ContextChipBase):
    """Chip widget for text context items."""

    def __init__(
        self,
        index: int,
        text_content: str,
        parent: QWidget | None = None,
    ):
        super().__init__(index, parent)
        self.full_text = text_content

        # Truncate text for display (max 30 chars)
        display_text = text_content.replace("\n", " ")
        if len(display_text) > 30:
            display_text = display_text[:27] + "..."
        self.set_label_text(display_text)

        # Set tooltip with full text
        self.setToolTip(text_content)

    def copy_to_clipboard(self):
        """Copy text content to clipboard."""
        clipboard = QApplication.clipboard()
        clipboard.setText(self.full_text)


class ImageContextChip(ContextChipBase):
    """Chip widget for image context items."""

    def __init__(
        self,
        index: int,
        image_number: int,
        image_data: str,
        media_type: str,
        parent: QWidget | None = None,
    ):
        super().__init__(index, parent)
        self.image_data = image_data
        self.media_type = media_type

        self.set_label_text(f"[image #{image_number}]")

        # Create tooltip with thumbnail preview
        self._setup_image_tooltip()

    def _setup_image_tooltip(self):
        """Set up tooltip with image thumbnail and metadata."""
        try:
            # Decode base64 image data
            image_bytes = base64.b64decode(self.image_data)
            image = QImage()
            image.loadFromData(QByteArray(image_bytes))

            if image.isNull():
                self.setToolTip("Image preview unavailable")
                return

            # Get original dimensions
            orig_width = image.width()
            orig_height = image.height()

            # Scale to thumbnail (max 300px)
            thumbnail = image.scaled(
                300,
                300,
                Qt.KeepAspectRatio,
                Qt.SmoothTransformation,
            )

            # Convert to base64 PNG for HTML tooltip
            buffer = QBuffer()
            buffer.open(QBuffer.WriteOnly)
            thumbnail.save(buffer, "PNG")
            thumb_base64 = base64.b64encode(buffer.data()).decode("utf-8")
            buffer.close()

            # Get format from media type
            format_name = self.media_type.split("/")[-1].upper()

            # Create HTML tooltip
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

    def copy_to_clipboard(self):
        """Copy image to clipboard."""
        try:
            image_bytes = base64.b64decode(self.image_data)
            image = QImage()
            image.loadFromData(QByteArray(image_bytes))

            if not image.isNull():
                clipboard = QApplication.clipboard()
                clipboard.setImage(image)
        except Exception as e:
            logger.warning(f"Failed to copy image to clipboard: {e}")


class ContextHeaderWidget(QWidget):
    """Header widget with 'Context' label, edit, copy and clear buttons."""

    clear_requested = Signal()
    copy_requested = Signal()
    edit_requested = Signal()
    set_context_requested = Signal()
    append_context_requested = Signal()

    _header_style = """
        QWidget {
            background: transparent;
        }
    """

    _title_style = """
        QLabel {
            color: #888888;
            font-size: 11px;
            font-weight: bold;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 22px;
            max-width: 22px;
            min-height: 22px;
            max-height: 22px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setStyleSheet(self._header_style)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 2)
        layout.setSpacing(4)

        title_label = QLabel("Context")
        title_label.setStyleSheet(self._title_style)
        layout.addWidget(title_label)

        layout.addStretch()

        # Set context from clipboard button
        self.set_btn = IconButton("file-symlink", size=18)
        self.set_btn.setStyleSheet(self._btn_style)
        self.set_btn.setCursor(Qt.PointingHandCursor)
        self.set_btn.setToolTip("Set context from clipboard")
        self.set_btn.clicked.connect(self._on_set_clicked)
        self.set_btn.setEnabled(False)  # Disabled by default
        layout.addWidget(self.set_btn)

        # Append clipboard to context button
        self.append_btn = IconButton("file-plus", size=18)
        self.append_btn.setStyleSheet(self._btn_style)
        self.append_btn.setCursor(Qt.PointingHandCursor)
        self.append_btn.setToolTip("Append clipboard to context")
        self.append_btn.clicked.connect(self._on_append_clicked)
        self.append_btn.setEnabled(False)  # Disabled by default
        layout.addWidget(self.append_btn)

        # Edit button
        self.edit_btn = IconButton("edit", size=18)
        self.edit_btn.setStyleSheet(self._btn_style)
        self.edit_btn.setCursor(Qt.PointingHandCursor)
        self.edit_btn.setToolTip("Edit context")
        self.edit_btn.clicked.connect(self._on_edit_clicked)
        layout.addWidget(self.edit_btn)

        # Copy button
        self.copy_btn = IconButton("copy", size=18)
        self.copy_btn.setStyleSheet(self._btn_style)
        self.copy_btn.setCursor(Qt.PointingHandCursor)
        self.copy_btn.setToolTip("Copy context text")
        self.copy_btn.clicked.connect(self._on_copy_clicked)
        self.copy_btn.setEnabled(False)  # Disabled by default
        layout.addWidget(self.copy_btn)

        # Clear button
        self.clear_btn = IconButton("trash", size=18)
        self.clear_btn.setStyleSheet(self._btn_style)
        self.clear_btn.setCursor(Qt.PointingHandCursor)
        self.clear_btn.setToolTip("Clear all context")
        self.clear_btn.clicked.connect(self._on_clear_clicked)
        self.clear_btn.setEnabled(False)  # Disabled by default
        layout.addWidget(self.clear_btn)

    def _on_clear_clicked(self):
        """Handle clear button click."""
        self.clear_requested.emit()

    def _on_copy_clicked(self):
        """Handle copy button click."""
        self.copy_requested.emit()

    def _on_edit_clicked(self):
        """Handle edit button click."""
        self.edit_requested.emit()

    def _on_set_clicked(self):
        """Handle set context button click."""
        self.set_context_requested.emit()

    def _on_append_clicked(self):
        """Handle append context button click."""
        self.append_context_requested.emit()

    def set_copy_enabled(self, enabled: bool):
        """Enable or disable the copy button."""
        self.copy_btn.setEnabled(enabled)

    def set_clear_enabled(self, enabled: bool):
        """Enable or disable the clear button."""
        self.clear_btn.setEnabled(enabled)

    def set_clipboard_buttons_enabled(self, enabled: bool):
        """Enable or disable the set and append clipboard buttons."""
        self.set_btn.setEnabled(enabled)
        self.append_btn.setEnabled(enabled)


class FlowLayout(QVBoxLayout):
    """Simple flow-like layout using horizontal layouts that wrap."""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setContentsMargins(4, 0, 4, 0)
        self.setSpacing(2)
        self._rows = []

    def clear_widgets(self):
        """Remove all widgets from the layout."""
        while self.count():
            item = self.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                self._clear_layout(item.layout())
        self._rows = []

    def _clear_layout(self, layout):
        """Recursively clear a layout."""
        while layout.count():
            item = layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                self._clear_layout(item.layout())

    def _current_row_full(self) -> bool:
        """Check if current row is full (simple heuristic: max 3 chips)."""
        if not self._rows:
            return True
        return self._rows[-1].count() >= 3

    def _add_new_row(self):
        """Add a new horizontal row."""
        row = QHBoxLayout()
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(4)
        row.addStretch()  # This will be at the end
        self.addLayout(row)
        self._rows.append(row)

    def add_widget(self, widget: QWidget):
        """Add a widget to the flow layout."""
        if not self._rows or self._current_row_full():
            self._add_new_row()

        # Insert widget before the stretch
        row = self._rows[-1]
        row.insertWidget(row.count() - 1, widget)


class ContextSectionWidget(QWidget):
    """Container widget for the context section in the menu."""

    context_changed = Signal()

    _container_style = """
        QWidget#contextSection {
            background: transparent;
        }
    """

    def __init__(
        self,
        context_manager: ContextManager,
        copy_callback: Callable[[], None] | None = None,
        notification_manager=None,
        clipboard_manager=None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.context_manager = context_manager
        self.copy_callback = copy_callback
        self.notification_manager = notification_manager
        self.clipboard_manager = clipboard_manager
        self._chips = []  # Store references to chips
        self.setObjectName("contextSection")
        self.setStyleSheet(self._container_style)

        self._setup_ui()
        self._rebuild_chips()

        # Subscribe to context changes
        self.context_manager.add_change_callback(self._on_context_changed)

        # Safety net: clean up on destruction
        self.destroyed.connect(self._safe_cleanup)

    def _setup_ui(self):
        """Set up the widget UI."""
        self.main_layout = QVBoxLayout(self)
        self.main_layout.setContentsMargins(0, 0, 0, 0)
        self.main_layout.setSpacing(2)

        # Header with title, edit, copy and clear buttons
        self.header = ContextHeaderWidget()
        self.header.clear_requested.connect(self._on_clear_all)
        self.header.copy_requested.connect(self._on_copy_text)
        self.header.edit_requested.connect(self._on_edit_context)
        self.header.set_context_requested.connect(self._on_set_from_clipboard)
        self.header.append_context_requested.connect(self._on_append_from_clipboard)
        self.main_layout.addWidget(self.header)

        # Container for chips
        self.chips_container = QWidget()
        self.chips_layout = FlowLayout()
        self.chips_container.setLayout(self.chips_layout)

        # Wrap chips in scroll area for overflow handling
        self.scroll_area = QScrollArea()
        self.scroll_area.setWidget(self.chips_container)
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.scroll_area.setMaximumHeight(150)
        self.scroll_area.setStyleSheet("""
            QScrollArea {
                background: transparent;
                border: none;
            }
            QScrollArea > QWidget > QWidget {
                background: transparent;
            }
            QScrollBar:vertical {
                background: #2a2a2a;
                width: 8px;
                border-radius: 4px;
            }
            QScrollBar::handle:vertical {
                background: #555555;
                border-radius: 4px;
                min-height: 20px;
            }
            QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
                height: 0px;
            }
        """)
        self.main_layout.addWidget(self.scroll_area)

    def _rebuild_chips(self):
        """Rebuild all chips from current context items."""
        self.chips_layout.clear_widgets()
        self._chips = []  # Store references to chips for copy handling

        items = self.context_manager.get_items()

        # Update header button states
        has_text = self.context_manager.has_context()
        has_items = bool(items)
        self.header.set_copy_enabled(has_text)
        self.header.set_clear_enabled(has_items)
        self._update_clipboard_button_state()

        if not items:
            # Show "No context" label when empty
            empty_label = QLabel("No context items")
            empty_label.setStyleSheet("QLabel { color: #666666; font-size: 11px; padding: 4px 8px; }")
            self.chips_layout.addWidget(empty_label)
            return

        # Separate images and text items, preserving original indices
        image_items = [(idx, item) for idx, item in enumerate(items) if item.item_type == ContextItemType.IMAGE]
        text_items = [(idx, item) for idx, item in enumerate(items) if item.item_type == ContextItemType.TEXT]

        # Display images first (with ascending numbering), then text
        for image_number, (idx, item) in enumerate(image_items, start=1):
            chip = ImageContextChip(
                index=idx,
                image_number=image_number,
                image_data=item.data or "",
                media_type=item.media_type or "image/png",
            )
            chip.delete_requested.connect(self._on_chip_delete)
            chip.copy_requested.connect(self._on_chip_copy)
            self._chips.append(chip)
            self.chips_layout.add_widget(chip)

        # Add spacing between images and text if both exist
        if image_items and text_items:
            spacer = QWidget()
            spacer.setFixedHeight(4)
            self.chips_layout.addWidget(spacer)

        for idx, item in text_items:
            chip = TextContextChip(
                index=idx,
                text_content=item.content or "",
            )
            chip.delete_requested.connect(self._on_chip_delete)
            chip.copy_requested.connect(self._on_chip_copy)
            self._chips.append(chip)
            self.chips_layout.add_widget(chip)

    def _on_context_changed(self):
        """Handle context manager change notification."""
        # Rebuild chips on the main thread
        self._rebuild_chips()
        self.context_changed.emit()

    def _on_chip_delete(self, index: int):
        """Handle chip delete request."""
        self.context_manager.remove_item(index)

    def _on_chip_copy(self, index: int):
        """Handle chip copy request."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        for chip in self._chips:
            if chip.index == index:
                chip.copy_to_clipboard()
                flash_confirmation(chip.copy_btn)
                break

    def _on_clear_all(self):
        """Handle clear all request."""
        self.context_manager.clear_context()

    def _on_copy_text(self):
        """Handle copy text request from header button."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        text_content = self.context_manager.get_context()
        if text_content:
            clipboard = QApplication.clipboard()
            clipboard.setText(text_content)
            flash_confirmation(self.header.copy_btn)

    def _on_edit_context(self):
        """Handle edit context request - open the context editor dialog."""
        if self.clipboard_manager is None:
            logger.warning("Cannot open context editor: clipboard_manager not available")
            return

        from modules.gui.context_editor_dialog import show_context_editor

        show_context_editor(
            self.context_manager,
            self.clipboard_manager,
            self.notification_manager,
        )

    def _on_set_from_clipboard(self):
        """Handle set context from clipboard request."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        if self.clipboard_manager is None:
            logger.warning("Cannot set context: clipboard_manager not available")
            return

        success = False
        try:
            if self.clipboard_manager.has_image():
                image_data = self.clipboard_manager.get_image_data()
                if image_data:
                    base64_data, media_type = image_data
                    self.context_manager.set_context_image(base64_data, media_type)
                    success = True
        except Exception as e:
            logger.debug(f"Failed to get clipboard image: {e}")

        if not success:
            try:
                text = self.clipboard_manager.get_content()
                if text and text.strip():
                    self.context_manager.set_context(text)
                    success = True
            except Exception as e:
                logger.warning(f"Failed to get clipboard text: {e}")

        if success:
            flash_confirmation(self.header.set_btn)

    def _on_append_from_clipboard(self):
        """Handle append clipboard to context request."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        if self.clipboard_manager is None:
            logger.warning("Cannot append context: clipboard_manager not available")
            return

        success = False
        try:
            if self.clipboard_manager.has_image():
                image_data = self.clipboard_manager.get_image_data()
                if image_data:
                    base64_data, media_type = image_data
                    self.context_manager.append_context_image(base64_data, media_type)
                    success = True
        except Exception as e:
            logger.debug(f"Failed to get clipboard image: {e}")

        if not success:
            try:
                text = self.clipboard_manager.get_content()
                if text and text.strip():
                    self.context_manager.append_context(text)
                    success = True
            except Exception as e:
                logger.warning(f"Failed to get clipboard text: {e}")

        if success:
            flash_confirmation(self.header.append_btn)

    def _has_clipboard_content(self) -> bool:
        """Check if clipboard has any content (text or image).

        Uses Qt clipboard directly to avoid X11 deadlock that occurs when
        subprocess calls (xclip/xsel) are made while Qt owns the clipboard.
        """
        try:
            from PySide6.QtWidgets import QApplication

            clipboard = QApplication.clipboard()
            mime_data = clipboard.mimeData()
            if mime_data:
                if mime_data.hasImage():
                    return True
                if mime_data.hasText() and mime_data.text().strip():
                    return True
        except Exception:
            pass
        return False

    def _update_clipboard_button_state(self):
        """Update the enabled state of clipboard buttons based on clipboard content."""
        has_content = self._has_clipboard_content()
        self.header.set_clipboard_buttons_enabled(has_content)

    def showEvent(self, event):
        """Handle show event - update clipboard button state."""
        super().showEvent(event)
        self._update_clipboard_button_state()

    def cleanup(self):
        """Clean up resources."""
        with contextlib.suppress(Exception):
            self.context_manager.remove_change_callback(self._on_context_changed)

    def _safe_cleanup(self):
        """Safety cleanup on widget destruction."""
        with contextlib.suppress(Exception):
            self.context_manager.remove_change_callback(self._on_context_changed)


class LastInteractionChip(QWidget):
    """Chip widget for last interaction items (input/output/transcription)."""

    copy_requested = Signal()
    details_requested = Signal()

    _chip_style = """
        QWidget#lastInteractionChip {
            background-color: #3a3a3a;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _chip_hover_style = """
        QWidget#lastInteractionChip {
            background-color: #454545;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _chip_disabled_style = """
        QWidget#lastInteractionChip {
            background-color: #2a2a2a;
            border: 1px solid #444444;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _label_style = """
        QLabel {
            color: #f0f0f0;
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _label_disabled_style = """
        QLabel {
            color: #666666;
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _icon_btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 20px;
            max-width: 20px;
            min-height: 20px;
            max-height: 20px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(
        self,
        chip_type: str,
        content: str | None,
        title: str,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.chip_type = chip_type
        self.content = content
        self.title = title
        self._enabled = content is not None and len(content) > 0

        self.setObjectName("lastInteractionChip")
        self.setAttribute(Qt.WA_StyledBackground, True)

        if self._enabled:
            self.setStyleSheet(self._chip_style)
            self.setCursor(Qt.PointingHandCursor)
        else:
            self.setStyleSheet(self._chip_disabled_style)
            self.setCursor(Qt.ArrowCursor)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(6, 2, 4, 2)
        layout.setSpacing(4)

        # Copy icon at the beginning
        self.copy_btn = IconButton("copy", size=16)
        self.copy_btn.setStyleSheet(self._icon_btn_style)
        self.copy_btn.setCursor(Qt.PointingHandCursor if self._enabled else Qt.ArrowCursor)
        self.copy_btn.setToolTip("Copy to clipboard")
        self.copy_btn.setEnabled(self._enabled)
        self.copy_btn.clicked.connect(self._on_copy_clicked)
        layout.addWidget(self.copy_btn)

        # Label with type name
        self.label = QLabel()
        self.label.setStyleSheet(self._label_style if self._enabled else self._label_disabled_style)
        self._set_display_text()
        layout.addWidget(self.label)

        # Details button (preview icon) at the end
        self.details_btn = IconButton("preview", size=16)
        self.details_btn.setStyleSheet(self._icon_btn_style)
        self.details_btn.setCursor(Qt.PointingHandCursor if self._enabled else Qt.ArrowCursor)
        self.details_btn.setToolTip("Show details")
        self.details_btn.setEnabled(self._enabled)
        self.details_btn.clicked.connect(self._on_details_clicked)
        layout.addWidget(self.details_btn)

        self.setSizePolicy(QSizePolicy.Maximum, QSizePolicy.Fixed)

        # Set tooltip
        if self._enabled:
            self.setToolTip(self.content)
        else:
            self.setToolTip("No content available")

    def _set_display_text(self):
        """Set the display text for the label."""
        self.label.setText(self.chip_type.capitalize())

    def _on_copy_clicked(self):
        """Handle copy button click."""
        if self._enabled:
            self.copy_requested.emit()

    def _on_details_clicked(self):
        """Handle details button click."""
        if self._enabled:
            self.details_requested.emit()

    def mousePressEvent(self, event):
        """Handle mouse press - copy on click (except on buttons)."""
        if not self._enabled:
            super().mousePressEvent(event)
            return

        # Check if click is on button areas
        details_btn_rect = self.details_btn.geometry()
        copy_btn_rect = self.copy_btn.geometry()

        if not details_btn_rect.contains(event.pos()) and not copy_btn_rect.contains(event.pos()):
            self._on_copy_clicked()
        super().mousePressEvent(event)

    def enterEvent(self, event):
        """Handle mouse enter - show hover state."""
        if self._enabled:
            self.setStyleSheet(self._chip_hover_style)
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Handle mouse leave - restore normal state."""
        if self._enabled:
            self.setStyleSheet(self._chip_style)
        else:
            self.setStyleSheet(self._chip_disabled_style)
        super().leaveEvent(event)

    def copy_to_clipboard(self):
        """Copy chip content to clipboard."""
        if self.content:
            clipboard = QApplication.clipboard()
            clipboard.setText(self.content)


class LastInteractionHeaderWidget(QWidget):
    """Header widget with 'Last interaction' label and history button."""

    history_requested = Signal()

    _header_style = """
        QWidget {
            background: transparent;
        }
    """

    _title_style = """
        QLabel {
            color: #888888;
            font-size: 11px;
            font-weight: bold;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 22px;
            max-width: 22px;
            min-height: 22px;
            max-height: 22px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setStyleSheet(self._header_style)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 2)
        layout.setSpacing(4)

        title_label = QLabel("Last interaction")
        title_label.setStyleSheet(self._title_style)
        layout.addWidget(title_label)

        layout.addStretch()

        self.history_btn = IconButton("history", size=18)
        self.history_btn.setStyleSheet(self._btn_style)
        self.history_btn.setCursor(Qt.PointingHandCursor)
        self.history_btn.setToolTip("View execution history")
        self.history_btn.clicked.connect(self._on_history_clicked)
        layout.addWidget(self.history_btn)

    def _on_history_clicked(self):
        self.history_requested.emit()


class LastInteractionSectionWidget(QWidget):
    """Container widget for the last interaction section in the menu."""

    # Signal for thread-safe history change notifications
    history_changed = Signal()

    _container_style = """
        QWidget#lastInteractionSection {
            background: transparent;
        }
    """

    def __init__(
        self,
        history_service,
        notification_manager=None,
        clipboard_manager=None,
        prompt_store_service=None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.history_service = history_service
        self.notification_manager = notification_manager
        self.clipboard_manager = clipboard_manager
        self.prompt_store_service = prompt_store_service
        self._chips = []

        self.setObjectName("lastInteractionSection")
        self.setStyleSheet(self._container_style)

        self._setup_ui()
        self._rebuild_chips()

        # Connect signal to rebuild chips (for thread-safe updates)
        self.history_changed.connect(self._rebuild_chips)

        # Subscribe to history changes
        if self.history_service:
            self.history_service.add_change_callback(self._on_history_changed)

        # Safety net: clean up on destruction
        self.destroyed.connect(self._safe_cleanup)

    def _setup_ui(self):
        """Set up the widget UI structure."""
        self.main_layout = QVBoxLayout(self)
        self.main_layout.setContentsMargins(0, 0, 0, 0)
        self.main_layout.setSpacing(2)

        # Header with title and history button
        header = LastInteractionHeaderWidget()
        header.history_requested.connect(self._on_history_requested)
        self.main_layout.addWidget(header)

        # Create container for chips (will be populated by _rebuild_chips)
        self.chips_container = QWidget()
        self.chips_layout = QHBoxLayout(self.chips_container)
        self.chips_layout.setContentsMargins(4, 0, 4, 4)
        self.chips_layout.setSpacing(6)
        self.main_layout.addWidget(self.chips_container)

    def _rebuild_chips(self):
        """Rebuild all chips from current history data."""
        from core.models import HistoryEntryType

        # Clear existing chips
        self._chips = []
        while self.chips_layout.count():
            item = self.chips_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        # Retrieve last interaction data
        last_text_entry = None
        last_speech_entry = None

        if self.history_service:
            last_text_entry = self.history_service.get_last_item_by_type(HistoryEntryType.TEXT)
            last_speech_entry = self.history_service.get_last_item_by_type(HistoryEntryType.SPEECH)

        input_content = last_text_entry.input_content if last_text_entry else None
        output_content = last_text_entry.output_content if last_text_entry else None
        transcription_content = last_speech_entry.output_content if last_speech_entry else None

        # Create chips
        input_chip = LastInteractionChip(
            chip_type="input",
            content=input_content,
            title="Input Content",
        )
        input_chip.copy_requested.connect(lambda: self._on_copy(input_chip))
        input_chip.details_requested.connect(lambda: self._on_details("Input Content", input_content))
        self._chips.append(input_chip)
        self.chips_layout.addWidget(input_chip)

        output_chip = LastInteractionChip(
            chip_type="output",
            content=output_content,
            title="Output Content",
        )
        output_chip.copy_requested.connect(lambda: self._on_copy(output_chip))
        output_chip.details_requested.connect(lambda: self._on_details("Output Content", output_content))
        self._chips.append(output_chip)
        self.chips_layout.addWidget(output_chip)

        transcription_chip = LastInteractionChip(
            chip_type="transcription",
            content=transcription_content,
            title="Transcription",
        )
        transcription_chip.copy_requested.connect(lambda: self._on_copy(transcription_chip))
        transcription_chip.details_requested.connect(lambda: self._on_details("Transcription", transcription_content))
        self._chips.append(transcription_chip)
        self.chips_layout.addWidget(transcription_chip)

        self.chips_layout.addStretch()

    def _on_history_changed(self):
        """Handle history service change notification.

        Emits signal to ensure UI update happens on the main thread,
        since this callback may be invoked from a background thread
        (e.g., after speech-to-text transcription completes).
        """
        self.history_changed.emit()

    def _on_copy(self, chip: LastInteractionChip):
        """Handle copy request from a chip."""
        from modules.gui.shared.icon_confirmation import flash_confirmation

        chip.copy_to_clipboard()
        flash_confirmation(chip.copy_btn)

    def _on_details(self, title: str, content: str | None):
        """Handle details request - show preview dialog."""
        if content:
            from modules.gui.text_preview_dialog import show_preview_dialog

            show_preview_dialog(title, content, clipboard_manager=self.clipboard_manager)

    def _on_history_requested(self):
        """Handle history button click - show history dialog."""
        from modules.gui.history_dialog import show_history_dialog

        show_history_dialog(
            history_service=self.history_service,
            clipboard_manager=self.clipboard_manager,
            prompt_store_service=self.prompt_store_service,
            notification_manager=self.notification_manager,
        )

    def _on_open_conversation(self, entry_id: str, prompt_id: str):
        """Handle opening a conversation from history."""
        if not self.prompt_store_service:
            logger.warning("Cannot open conversation: prompt_store_service is None")
            return

        if not entry_id:
            logger.warning("Cannot open conversation: entry_id is empty")
            return

        # Find the menu item for the prompt
        menu_item = None
        all_items = self.prompt_store_service.get_all_available_prompts()

        if prompt_id:
            for item in all_items:
                item_prompt_id = item.data.get("prompt_id") if item.data else None
                if item_prompt_id == prompt_id:
                    menu_item = item
                    break

        # Fallback to first available prompt if exact match not found
        if not menu_item and all_items:
            menu_item = all_items[0]
            logger.info(f"Using fallback prompt for conversation {entry_id} (original prompt_id={prompt_id})")

        if not menu_item:
            logger.warning(f"Cannot open conversation: no prompts available")
            return

        logger.debug(f"Opening conversation {entry_id} for prompt {prompt_id}")

        # Open the prompt execute dialog with history restoration
        from modules.gui.prompt_execute_dialog import show_prompt_execute_dialog

        context_manager = None
        if hasattr(self.prompt_store_service, "context_manager"):
            context_manager = self.prompt_store_service.context_manager

        show_prompt_execute_dialog(
            menu_item,
            lambda item, shift: None,  # Execution callback - not needed for restore
            prompt_store_service=self.prompt_store_service,
            context_manager=context_manager,
            clipboard_manager=self.clipboard_manager,
            notification_manager=self.notification_manager,
            history_service=self.history_service,
            history_entry_id=entry_id,
        )

    def cleanup(self):
        """Clean up resources."""
        try:
            if self.history_service:
                self.history_service.remove_change_callback(self._on_history_changed)
        except Exception:
            pass

    def _safe_cleanup(self):
        """Safety cleanup on widget destruction."""
        try:
            if self.history_service:
                self.history_service.remove_change_callback(self._on_history_changed)
        except Exception:
            pass


class SettingsSelectorChip(QWidget):
    """Chip widget for selecting model or prompt with dropdown menu."""

    selection_changed = Signal(str)  # Emits selected item key
    clear_requested = Signal()  # Emits when clear button clicked

    _chip_style = """
        QWidget#settingsChip {
            background-color: #3a3a3a;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _chip_hover_style = """
        QWidget#settingsChip {
            background-color: #454545;
            border: 1px solid #555555;
            border-radius: 12px;
            padding: 2px;
        }
    """

    _label_style = """
        QLabel {
            color: #f0f0f0;
            font-size: 12px;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _icon_btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 20px;
            max-width: 20px;
            min-height: 20px;
            max-height: 20px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(
        self,
        prefix: str,
        current_value: str,
        options: list,
        clearable: bool = False,
        on_select: Callable[[str], None] | None = None,
        on_clear: Callable[[], None] | None = None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.prefix = prefix
        self.current_value = current_value
        self.options = options  # List of MenuItem objects
        self.clearable = clearable
        self._on_select_callback = on_select
        self._on_clear_callback = on_clear

        self.setObjectName("settingsChip")
        self.setAttribute(Qt.WA_StyledBackground, True)
        self.setStyleSheet(self._chip_style)
        self.setCursor(Qt.PointingHandCursor)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 2, 4, 2)
        layout.setSpacing(4)

        # Label with prefix and value
        self.label = QLabel()
        self.label.setStyleSheet(self._label_style)
        self._update_label()
        layout.addWidget(self.label)

        # Clear button (only if clearable)
        if self.clearable:
            self.clear_btn = IconButton("delete", size=16)
            self.clear_btn.setStyleSheet(self._icon_btn_style)
            self.clear_btn.setToolTip(f"Clear {prefix}")
            self.clear_btn.clicked.connect(self._on_clear_clicked)
            # Enable only if there's a value selected (not "None")
            has_value = current_value and current_value != "None"
            self.clear_btn.setEnabled(has_value)
            if has_value:
                self.clear_btn.setCursor(Qt.PointingHandCursor)
            layout.addWidget(self.clear_btn)

        # Dropdown chevron
        self.chevron_btn = IconButton("chevron-down", size=16)
        self.chevron_btn.setStyleSheet(self._icon_btn_style)
        self.chevron_btn.setCursor(Qt.PointingHandCursor)
        self.chevron_btn.setToolTip(f"Select {prefix}")
        self.chevron_btn.clicked.connect(self._show_menu)
        layout.addWidget(self.chevron_btn)

        self.setSizePolicy(QSizePolicy.Maximum, QSizePolicy.Fixed)

    def _update_label(self):
        """Update the label with prefix and current value."""
        prefix_html = f'<i style="color: #999999">{self.prefix}:</i>'
        # Truncate long values
        max_length = 20
        display_value = self.current_value
        if len(display_value) > max_length:
            display_value = display_value[: max_length - 3] + "..."
            self.setToolTip(f"{self.prefix}: {self.current_value}")
        else:
            self.setToolTip("")
        self.label.setText(f"{prefix_html} {display_value}")

    def update_value(self, new_value: str):
        """Update the displayed value."""
        self.current_value = new_value
        self._update_label()
        # Update clear button state if it exists
        if self.clearable and hasattr(self, "clear_btn"):
            has_value = new_value and new_value != "None"
            self.clear_btn.setEnabled(has_value)
            self.clear_btn.setCursor(Qt.PointingHandCursor if has_value else Qt.ArrowCursor)

    def update_options(self, options: list):
        """Update the available options."""
        self.options = options

    def _on_clear_clicked(self):
        """Handle clear button click."""
        # Update displayed value immediately
        self.update_value("None")

        self.clear_requested.emit()
        if self._on_clear_callback:
            self._on_clear_callback()

    def _show_menu(self):
        """Show dropdown menu with options."""
        from PySide6.QtWidgets import QMenu

        menu = QMenu(self)
        menu.setStyleSheet("""
            QMenu {
                background-color: #2b2b2b;
                border: 1px solid #555555;
                border-radius: 6px;
                padding: 4px;
                color: #ffffff;
                font-size: 13px;
            }
            QMenu::item {
                background-color: transparent;
                padding: 8px 16px;
                border-radius: 4px;
                margin: 1px;
            }
            QMenu::item:selected {
                background-color: #454545;
                color: #ffffff;
            }
            QMenu::item:disabled {
                color: #666666;
            }
        """)

        for option in self.options:
            action = menu.addAction(option.label)
            action.setEnabled(option.enabled)
            # Capture the option in the lambda
            action.triggered.connect(lambda checked, opt=option: self._on_option_selected(opt))

        # Show menu below the chip
        menu.exec_(self.mapToGlobal(QPoint(0, self.height())))

    def _on_option_selected(self, option):
        """Handle option selection from menu."""
        # Update displayed value immediately (remove checkmark prefix if present)
        display_name = option.label
        if display_name.startswith("✓ "):
            display_name = display_name[2:]
        self.update_value(display_name)

        self.selection_changed.emit(option.id)
        if option.action:
            option.action()
        if self._on_select_callback:
            self._on_select_callback(option.id)

    def mousePressEvent(self, event):
        """Handle mouse press - show menu on click (except on buttons)."""
        if self.clearable and hasattr(self, "clear_btn"):
            clear_btn_rect = self.clear_btn.geometry()
            if clear_btn_rect.contains(event.pos()):
                super().mousePressEvent(event)
                return

        chevron_rect = self.chevron_btn.geometry()
        if not chevron_rect.contains(event.pos()):
            self._show_menu()
        super().mousePressEvent(event)

    def enterEvent(self, event):
        """Handle mouse enter - show hover state."""
        self.setStyleSheet(self._chip_hover_style)
        super().enterEvent(event)

    def leaveEvent(self, event):
        """Handle mouse leave - restore normal state."""
        self.setStyleSheet(self._chip_style)
        super().leaveEvent(event)


class SettingsHeaderWidget(QWidget):
    """Header widget with 'Settings' label and settings/close buttons."""

    settings_requested = Signal()
    close_app_requested = Signal()

    _header_style = """
        QWidget {
            background: transparent;
        }
    """

    _title_style = """
        QLabel {
            color: #888888;
            font-size: 11px;
            font-weight: bold;
            padding: 2px 4px;
            background: transparent;
        }
    """

    _btn_style = (
        """
        QPushButton {
            background: transparent;
            border: none;
            padding: 2px;
            min-width: 22px;
            max-width: 22px;
            min-height: 22px;
            max-height: 22px;
        }
    """
        + TOOLTIP_STYLE
    )

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setStyleSheet(self._header_style)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 2)
        layout.setSpacing(4)

        title = QLabel("Settings")
        title.setStyleSheet(self._title_style)
        layout.addWidget(title)
        layout.addStretch()

        self.settings_btn = IconButton("settings", size=18)
        self.settings_btn.setStyleSheet(self._btn_style)
        self.settings_btn.setCursor(Qt.PointingHandCursor)
        self.settings_btn.setToolTip("Settings")
        self.settings_btn.clicked.connect(self._on_settings_clicked)
        layout.addWidget(self.settings_btn)

        self.close_app_btn = IconButton("power", size=18)
        self.close_app_btn.setStyleSheet(self._btn_style)
        self.close_app_btn.setCursor(Qt.PointingHandCursor)
        self.close_app_btn.setToolTip("Close App")
        self.close_app_btn.clicked.connect(self._on_close_app_clicked)
        layout.addWidget(self.close_app_btn)

    def _on_settings_clicked(self):
        self.settings_requested.emit()

    def _on_close_app_clicked(self):
        self.close_app_requested.emit()


class SettingsSectionWidget(QWidget):
    """Section widget containing settings chips for model and prompt selection."""

    model_changed = Signal(str)  # Emits new model key
    prompt_changed = Signal(str)  # Emits new prompt id
    prompt_cleared = Signal()  # Emits when prompt is cleared

    _section_style = """
        QWidget {
            background: transparent;
        }
    """

    def __init__(
        self,
        model_options: list,
        prompt_options: list,
        current_model: str,
        current_prompt: str,
        on_model_select: Callable[[str], None] | None = None,
        on_prompt_select: Callable[[str], None] | None = None,
        on_prompt_clear: Callable[[], None] | None = None,
        on_settings_click: Callable[[], None] | None = None,
        on_close_app_click: Callable[[], None] | None = None,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self.setStyleSheet(self._section_style)

        self._on_model_select = on_model_select
        self._on_prompt_select = on_prompt_select
        self._on_prompt_clear = on_prompt_clear
        self._on_settings_click = on_settings_click
        self._on_close_app_click = on_close_app_click

        main_layout = QVBoxLayout(self)
        main_layout.setContentsMargins(4, 0, 4, 4)
        main_layout.setSpacing(4)

        # Header
        header = SettingsHeaderWidget()
        header.settings_requested.connect(self._handle_settings_click)
        header.close_app_requested.connect(self._handle_close_app_click)
        main_layout.addWidget(header)

        # Chips row
        chips_layout = QHBoxLayout()
        chips_layout.setContentsMargins(4, 0, 4, 0)
        chips_layout.setSpacing(8)

        # Model chip (not clearable)
        self.model_chip = SettingsSelectorChip(
            prefix="model",
            current_value=current_model,
            options=model_options,
            clearable=False,
            on_select=self._handle_model_select,
        )
        chips_layout.addWidget(self.model_chip)

        # Prompt chip (clearable)
        self.prompt_chip = SettingsSelectorChip(
            prefix="prompt",
            current_value=current_prompt,
            options=prompt_options,
            clearable=True,
            on_select=self._handle_prompt_select,
            on_clear=self._handle_prompt_clear,
        )
        chips_layout.addWidget(self.prompt_chip)

        chips_layout.addStretch()
        main_layout.addLayout(chips_layout)

    def _handle_model_select(self, model_key: str):
        """Handle model selection."""
        self.model_changed.emit(model_key)
        if self._on_model_select:
            self._on_model_select(model_key)

    def _handle_prompt_select(self, prompt_id: str):
        """Handle prompt selection."""
        self.prompt_changed.emit(prompt_id)
        if self._on_prompt_select:
            self._on_prompt_select(prompt_id)

    def _handle_prompt_clear(self):
        """Handle prompt clear."""
        self.prompt_cleared.emit()
        if self._on_prompt_clear:
            self._on_prompt_clear()

    def _handle_settings_click(self):
        """Handle settings button click."""
        if self._on_settings_click:
            self._on_settings_click()

    def _handle_close_app_click(self):
        """Handle close app button click."""
        if self._on_close_app_click:
            self._on_close_app_click()

    def update_model(self, model_name: str, options: list):
        """Update model chip with new value and options."""
        self.model_chip.update_value(model_name)
        self.model_chip.update_options(options)

    def update_prompt(self, prompt_name: str, options: list):
        """Update prompt chip with new value and options."""
        self.prompt_chip.update_value(prompt_name)
        self.prompt_chip.update_options(options)
