"""Inline widget for editing a single prompt configuration."""

import logging
import re
import uuid

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMessageBox,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from modules.gui.shared.context_widgets import IconButton
from modules.gui.shared.widgets import NoScrollComboBox
from modules.gui.shared.theme import (
    COLOR_BORDER,
    COLOR_TEXT,
    COLOR_TEXT_EDIT_BG,
    COLOR_DIALOG_BG,
    SVG_CHEVRON_DOWN_PATH,
    TOOLTIP_STYLE,
)

from .prompt_editor_dialog import DescriptionGeneratorWorker, PlaceholderHighlighter

logger = logging.getLogger(__name__)

MIN_CONTENT_LENGTH = 10

FORM_STYLE = f"""
    QLineEdit {{
        background-color: {COLOR_TEXT_EDIT_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 6px 10px;
    }}
    QTextEdit {{
        background-color: {COLOR_TEXT_EDIT_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 8px;
        font-family: "Menlo", "Monaco", "Consolas", monospace;
        font-size: 12px;
    }}
    QLabel {{
        color: {COLOR_TEXT};
    }}
    QComboBox {{
        background-color: {COLOR_TEXT_EDIT_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 6px 10px;
        min-width: 140px;
    }}
    QComboBox::drop-down {{
        border: none;
        width: 20px;
    }}
    QComboBox::down-arrow {{
        image: url("{SVG_CHEVRON_DOWN_PATH}");
        width: 12px;
        height: 12px;
    }}
    QComboBox QAbstractItemView {{
        background-color: {COLOR_DIALOG_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
    }}
"""


class PromptEditorWidget(QWidget):
    """Inline widget for editing a prompt's configuration."""

    changed = Signal()

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self._prompt_id: str | None = None
        self._is_new: bool = False
        self._generator_worker: DescriptionGeneratorWorker | None = None
        self._setup_ui()

    def _setup_ui(self):
        self.setStyleSheet(FORM_STYLE)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self._form_container = QWidget()
        container_layout = QVBoxLayout(self._form_container)
        container_layout.setContentsMargins(0, 0, 0, 0)
        container_layout.setSpacing(12)

        title = QLabel("Prompt Configuration")
        title.setStyleSheet(f"""
            QLabel {{
                color: {COLOR_TEXT};
                font-size: 14px;
                font-weight: bold;
            }}
        """)
        container_layout.addWidget(title)

        name_row = QHBoxLayout()
        name_label = QLabel("Name:")
        name_label.setFixedWidth(100)
        self._name_edit = QLineEdit()
        self._name_edit.setPlaceholderText("Prompt name")
        name_row.addWidget(name_label)
        name_row.addWidget(self._name_edit)
        container_layout.addLayout(name_row)

        model_row = QHBoxLayout()
        model_label = QLabel("Model (optional):")
        model_label.setFixedWidth(100)
        self._model_combo = NoScrollComboBox()
        self._model_combo.addItem("Use Default", "")
        self._populate_models()
        model_row.addWidget(model_label)
        model_row.addWidget(self._model_combo)
        container_layout.addLayout(model_row)

        placeholder_info_label = QLabel(self._build_placeholder_info_text())
        placeholder_info_label.setStyleSheet("""
            QLabel {
                color: #888888;
                font-size: 11px;
                padding: 4px 0;
            }
        """)
        placeholder_info_label.setWordWrap(True)
        container_layout.addWidget(placeholder_info_label)

        system_label = QLabel("System Message:")
        container_layout.addWidget(system_label)

        self._system_edit = QTextEdit()
        self._system_edit.setPlaceholderText("Enter system message content...")
        self._system_edit.setMinimumHeight(120)
        container_layout.addWidget(self._system_edit)

        user_label = QLabel("User Message Template:")
        container_layout.addWidget(user_label)

        self._user_edit = QTextEdit()
        self._user_edit.setPlaceholderText(
            "Enter user message template. Use {{clipboard}} and {{context}} placeholders."
        )
        self._user_edit.setMinimumHeight(100)
        container_layout.addWidget(self._user_edit)

        valid_names = set(self._get_placeholder_info().keys())
        PlaceholderHighlighter(valid_names, self._system_edit.document())
        PlaceholderHighlighter(valid_names, self._user_edit.document())

        description_label = QLabel("Description:")
        container_layout.addWidget(description_label)

        description_row = QHBoxLayout()
        description_row.setAlignment(Qt.AlignTop)
        self._description_edit = QTextEdit()
        self._description_edit.setPlaceholderText("Description (optional)")
        self._description_edit.setMinimumHeight(60)
        self._description_edit.setMaximumHeight(80)
        description_row.addWidget(self._description_edit)

        icon_btn_style = f"""
            QPushButton {{
                background: transparent;
                border: none;
                padding: 4px;
                min-width: 28px;
                max-width: 28px;
                min-height: 28px;
                max-height: 28px;
            }}
            {TOOLTIP_STYLE}
        """
        self._generate_btn = IconButton("bot", size=18)
        self._generate_btn.setStyleSheet(icon_btn_style)
        self._generate_btn.setToolTip("Generate description using AI")
        self._generate_btn.setEnabled(False)
        self._generate_btn.clicked.connect(self._on_generate_description)
        description_row.addWidget(self._generate_btn, alignment=Qt.AlignTop)
        container_layout.addLayout(description_row)

        self._system_edit.textChanged.connect(self._update_generate_button_state)
        self._user_edit.textChanged.connect(self._update_generate_button_state)

        self._name_edit.textChanged.connect(lambda _: self.changed.emit())
        self._model_combo.currentIndexChanged.connect(lambda _: self.changed.emit())
        self._system_edit.textChanged.connect(self.changed.emit)
        self._user_edit.textChanged.connect(self.changed.emit)
        self._description_edit.textChanged.connect(self.changed.emit)

        container_layout.addStretch()

        layout.addWidget(self._form_container)
        self._form_container.hide()

    def _populate_models(self):
        from modules.utils.config import ConfigService

        config_service = ConfigService()
        config = config_service.get_config()

        if config.models:
            for model in config.models:
                model_id = model.get("id")
                display_name = model.get("display_name", model_id)
                self._model_combo.addItem(display_name, model_id)

    def _load_file_content(self, file_path: str) -> str:
        from core.services import SettingsService

        try:
            settings_service = SettingsService()
            settings_service.load_settings()
            return settings_service._load_file_content(file_path)
        except Exception as e:
            logger.warning(f"Failed to load file {file_path}: {e}")
            return f"[Error loading file: {file_path}]"

    def _block_change_signals(self, block: bool):
        self._name_edit.blockSignals(block)
        self._model_combo.blockSignals(block)
        self._system_edit.blockSignals(block)
        self._user_edit.blockSignals(block)
        self._description_edit.blockSignals(block)

    def load_prompt(self, prompt_id: str, prompt_data: dict):
        self._prompt_id = prompt_id
        self._is_new = False
        self._form_container.show()

        self._block_change_signals(True)

        self._name_edit.setText(prompt_data.get("name", ""))
        self._description_edit.setPlainText(prompt_data.get("description", ""))

        model = prompt_data.get("model", "")
        for i in range(self._model_combo.count()):
            if self._model_combo.itemData(i) == model:
                self._model_combo.setCurrentIndex(i)
                break
        else:
            self._model_combo.setCurrentIndex(0)

        self._system_edit.clear()
        self._user_edit.clear()

        messages = prompt_data.get("messages", [])
        for msg in messages:
            role = msg.get("role", "")
            if role == "system":
                if "file" in msg:
                    content = self._load_file_content(msg["file"])
                    self._system_edit.setPlainText(content)
                elif "content" in msg:
                    self._system_edit.setPlainText(msg["content"])
            elif role == "user":
                self._user_edit.setPlainText(msg.get("content", ""))

        self._block_change_signals(False)

        self._update_generate_button_state()

    def clear(self):
        self._prompt_id = None
        self._is_new = False
        self._form_container.hide()

        self._block_change_signals(True)
        self._name_edit.clear()
        self._model_combo.setCurrentIndex(0)
        self._system_edit.clear()
        self._user_edit.clear()
        self._description_edit.clear()
        self._block_change_signals(False)

    def set_new_mode(self):
        self._prompt_id = str(uuid.uuid4())
        self._is_new = True
        self._form_container.show()

        self._block_change_signals(True)
        self._name_edit.clear()
        self._model_combo.setCurrentIndex(0)
        self._system_edit.clear()
        self._user_edit.setPlainText("{{clipboard}}")
        self._description_edit.clear()
        self._block_change_signals(False)

        self._name_edit.setFocus()
        self._update_generate_button_state()

    def get_prompt_data(self) -> tuple[str, dict] | None:
        name = self._name_edit.text().strip()
        if not name:
            return None

        invalid_placeholders = self._validate_placeholders()
        if invalid_placeholders:
            unique_invalid = list(dict.fromkeys(invalid_placeholders))
            placeholder_list = ", ".join(f"{{{{{p}}}}}" for p in unique_invalid)
            reply = QMessageBox.warning(
                self,
                "Invalid Placeholders",
                f"The following placeholders are not recognized:\n{placeholder_list}\n\n"
                "These will not be replaced during prompt execution.",
                QMessageBox.Save | QMessageBox.Cancel,
                QMessageBox.Cancel,
            )
            if reply != QMessageBox.Save:
                return None

        prompt_id = self._prompt_id or str(uuid.uuid4())

        messages = []
        system_content = self._system_edit.toPlainText().strip()
        if system_content:
            messages.append({"role": "system", "content": system_content})

        user_content = self._user_edit.toPlainText().strip()
        if user_content:
            messages.append({"role": "user", "content": user_content})

        result = {
            "id": prompt_id,
            "name": name,
            "messages": messages,
        }

        description = self._description_edit.toPlainText().strip()
        if description:
            result["description"] = description

        model = self._model_combo.currentData()
        if model:
            result["model"] = model

        return (prompt_id, result)

    def is_new_prompt(self) -> bool:
        return self._is_new

    def mark_saved(self):
        self._is_new = False

    def _get_combined_content_length(self) -> int:
        system_len = len(self._system_edit.toPlainText().strip())
        user_len = len(self._user_edit.toPlainText().strip())
        return system_len + user_len

    def _update_generate_button_state(self):
        content_len = self._get_combined_content_length()
        is_generating = self._generator_worker is not None and self._generator_worker.isRunning()

        if is_generating:
            self._generate_btn.setEnabled(False)
            self._generate_btn.setToolTip("Generating...")
        elif content_len >= MIN_CONTENT_LENGTH:
            self._generate_btn.setEnabled(True)
            self._generate_btn.setToolTip("Generate description using AI")
        else:
            chars_needed = MIN_CONTENT_LENGTH - content_len
            self._generate_btn.setEnabled(False)
            self._generate_btn.setToolTip(
                f"Enter at least {chars_needed} more character{'s' if chars_needed != 1 else ''}"
            )

    def _on_generate_description(self):
        if self._generator_worker is not None and self._generator_worker.isRunning():
            return

        name = self._name_edit.text().strip()
        system_content = self._system_edit.toPlainText().strip()
        user_content = self._user_edit.toPlainText().strip()
        placeholder_info = self._get_placeholder_info()

        self._generator_worker = DescriptionGeneratorWorker(
            name=name,
            system_content=system_content,
            user_content=user_content,
            placeholder_info=placeholder_info,
            parent=self,
        )
        self._generator_worker.finished.connect(self._on_generation_finished)
        self._generator_worker.error.connect(self._on_generation_error)
        self._generator_worker.start()

        self._update_generate_button_state()

    def _on_generation_finished(self, description: str):
        self._description_edit.setPlainText(description)
        self._update_generate_button_state()

    def _on_generation_error(self, error_msg: str):
        logger.warning(f"Description generation failed: {error_msg}")
        self._update_generate_button_state()

    def _get_placeholder_info(self) -> dict[str, str]:
        return {
            "clipboard": "The current clipboard text content",
            "context": "Persistent context data set across prompt executions",
        }

    def _build_placeholder_info_text(self) -> str:
        info = self._get_placeholder_info()
        lines = ["Available Placeholders:"]
        for name, description in info.items():
            lines.append(f"  {{{{{name}}}}} - {description}")
        return "\n".join(lines)

    def _find_invalid_placeholders(self, content: str) -> list[str]:
        pattern = r"\{\{(\w+)\}\}"
        found = re.findall(pattern, content)
        valid_names = set(self._get_placeholder_info().keys())
        return [name for name in found if name not in valid_names]

    def _validate_placeholders(self) -> list[str]:
        system_content = self._system_edit.toPlainText()
        user_content = self._user_edit.toPlainText()
        combined_content = f"{system_content}\n{user_content}"
        return self._find_invalid_placeholders(combined_content)
