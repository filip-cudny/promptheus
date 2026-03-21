"""Prompts settings panel."""

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSplitter,
    QVBoxLayout,
    QWidget,
)

from modules.gui.shared.context_widgets import IconButton
from modules.gui.shared.widgets import NoScrollComboBox
from modules.gui.shared.theme import (
    COLOR_BORDER,
    COLOR_BUTTON_BG,
    COLOR_BUTTON_HOVER,
    COLOR_DIALOG_BG,
    COLOR_TEXT,
    COLOR_TEXT_EDIT_BG,
    COLOR_TEXT_SECONDARY,
    SVG_CHEVRON_DOWN_PATH,
    TOOLTIP_STYLE,
)
from modules.utils.config import ConfigService

from ..settings_panel_base import SettingsPanelBase
from ..widgets.prompt_editor_widget import PromptEditorWidget
from ..widgets.prompt_list_widget import PromptListWidget
from ..widgets.prompt_template_dialog import PromptTemplateDialog

TOOLBAR_BTN_STYLE = (
    f"""
    QPushButton {{
        background-color: {COLOR_BUTTON_BG};
        color: {COLOR_TEXT};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 4px 12px;
        min-height: 28px;
        max-height: 28px;
    }}
    QPushButton:hover {{
        background-color: {COLOR_BUTTON_HOVER};
    }}
    QPushButton:disabled {{
        background-color: {COLOR_DIALOG_BG};
        color: #666666;
    }}
"""
    + TOOLTIP_STYLE
)

ICON_BTN_STYLE = (
    f"""
    QPushButton {{
        background-color: {COLOR_BUTTON_BG};
        border: 1px solid {COLOR_BORDER};
        border-radius: 4px;
        padding: 4px;
        min-width: 28px;
        max-width: 28px;
        min-height: 28px;
        max-height: 28px;
    }}
    QPushButton:hover {{
        background-color: {COLOR_BUTTON_HOVER};
    }}
    QPushButton:disabled {{
        background-color: {COLOR_DIALOG_BG};
    }}
"""
    + TOOLTIP_STYLE
)


class PromptsPanel(SettingsPanelBase):
    """Panel for managing prompts."""

    @property
    def panel_title(self) -> str:
        return "Prompts"

    def _setup_content(self, layout: QVBoxLayout) -> None:
        self._config_service = ConfigService()
        self._pending_generator_config = {}

        self._setup_generator_config_section(layout)
        self._setup_toolbar(layout)
        self._setup_splitter(layout)

        self._load_prompts()
        self._load_generator_config()

    def _setup_generator_config_section(self, layout: QVBoxLayout) -> None:
        container = QWidget()
        container.setStyleSheet(f"""
            QWidget {{
                background-color: {COLOR_DIALOG_BG};
            }}
            QLabel {{
                color: {COLOR_TEXT};
            }}
            QComboBox {{
                background-color: {COLOR_TEXT_EDIT_BG};
                color: {COLOR_TEXT};
                border: 1px solid {COLOR_BORDER};
                border-radius: 4px;
                padding: 6px 8px;
                padding-right: 28px;
                min-width: 200px;
            }}
            QComboBox::drop-down {{
                border: none;
                width: 24px;
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
        """)

        section_layout = QVBoxLayout(container)
        section_layout.setContentsMargins(0, 0, 0, 12)
        section_layout.setSpacing(8)

        title_label = QLabel("Description Generator")
        title_label.setStyleSheet(f"color: {COLOR_TEXT}; font-weight: bold;")
        section_layout.addWidget(title_label)

        row_layout = QHBoxLayout()
        row_layout.setSpacing(8)

        model_label = QLabel("Model:")
        row_layout.addWidget(model_label)

        self._generator_model_combo = NoScrollComboBox()
        self._generator_model_combo.addItem("Default (first available)", "")
        self._populate_generator_models()
        self._generator_model_combo.currentIndexChanged.connect(self._on_generator_model_changed)
        row_layout.addWidget(self._generator_model_combo)

        row_layout.addStretch()
        section_layout.addLayout(row_layout)

        prompt_row = QHBoxLayout()
        prompt_row.setSpacing(8)

        prompt_label = QLabel("Prompt:")
        prompt_row.addWidget(prompt_label)

        self._generator_prompt_label = QLabel()
        self._generator_prompt_label.setStyleSheet(f"color: {COLOR_TEXT_SECONDARY};")
        prompt_row.addWidget(self._generator_prompt_label)

        self._edit_prompt_btn = IconButton("edit", size=16)
        self._edit_prompt_btn.setStyleSheet(ICON_BTN_STYLE)
        self._edit_prompt_btn.setToolTip("Edit generator system prompt")
        self._edit_prompt_btn.setCursor(Qt.PointingHandCursor)
        self._edit_prompt_btn.clicked.connect(self._on_edit_generator_prompt)
        prompt_row.addWidget(self._edit_prompt_btn)

        prompt_row.addStretch()
        section_layout.addLayout(prompt_row)

        layout.addWidget(container)

    def _setup_toolbar(self, layout: QVBoxLayout) -> None:
        toolbar = QHBoxLayout()
        toolbar.setSpacing(8)

        self._add_btn = QPushButton("Add")
        self._add_btn.setStyleSheet(TOOLBAR_BTN_STYLE)
        self._add_btn.setToolTip("Add new prompt")
        self._add_btn.clicked.connect(self._on_add_prompt)
        toolbar.addWidget(self._add_btn)

        self._delete_btn = QPushButton("Delete")
        self._delete_btn.setStyleSheet(TOOLBAR_BTN_STYLE)
        self._delete_btn.setToolTip("Delete selected prompt")
        self._delete_btn.setEnabled(False)
        self._delete_btn.clicked.connect(self._on_delete_prompt)
        toolbar.addWidget(self._delete_btn)

        self._up_btn = IconButton("chevron-up", size=16)
        self._up_btn.setStyleSheet(ICON_BTN_STYLE)
        self._up_btn.setToolTip("Move up")
        self._up_btn.setEnabled(False)
        self._up_btn.clicked.connect(lambda: self._prompt_list.move_up())
        toolbar.addWidget(self._up_btn)

        self._down_btn = IconButton("chevron-down", size=16)
        self._down_btn.setStyleSheet(ICON_BTN_STYLE)
        self._down_btn.setToolTip("Move down")
        self._down_btn.setEnabled(False)
        self._down_btn.clicked.connect(lambda: self._prompt_list.move_down())
        toolbar.addWidget(self._down_btn)

        toolbar.addStretch()

        layout.addLayout(toolbar)

    def _setup_splitter(self, layout: QVBoxLayout) -> None:
        splitter = QSplitter(Qt.Horizontal)
        splitter.setStyleSheet(f"""
            QSplitter::handle {{
                background-color: {COLOR_BORDER};
                width: 1px;
            }}
        """)

        left_container = QWidget()
        left_layout = QVBoxLayout(left_container)
        left_layout.setContentsMargins(0, 0, 8, 0)

        self._prompt_list = PromptListWidget()
        self._prompt_list.prompt_selected.connect(self._on_prompt_selected)
        self._prompt_list.order_changed.connect(self._on_order_changed)
        left_layout.addWidget(self._prompt_list)
        splitter.addWidget(left_container)

        right_container = QWidget()
        right_layout = QVBoxLayout(right_container)
        right_layout.setContentsMargins(8, 0, 0, 0)

        self._prompt_editor = PromptEditorWidget()
        self._prompt_editor.changed.connect(self.mark_dirty)
        right_layout.addWidget(self._prompt_editor)
        splitter.addWidget(right_container)

        splitter.setSizes([200, 400])
        layout.addWidget(splitter, 1)

    def _populate_generator_models(self) -> None:
        config = self._config_service.get_config()
        if config.models:
            for model in config.models:
                model_id = model.get("id")
                display_name = model.get("display_name", model_id)
                self._generator_model_combo.addItem(display_name, model_id)

    def _load_generator_config(self) -> None:
        config = self._config_service.get_description_generator_config()
        model_id = config.get("model", "")

        for i in range(self._generator_model_combo.count()):
            if self._generator_model_combo.itemData(i) == model_id:
                self._generator_model_combo.setCurrentIndex(i)
                break

        self._update_generator_prompt_label()

    def _update_generator_prompt_label(self) -> None:
        config = self._config_service.get_description_generator_config()
        text = self._pending_generator_config.get("system_prompt", config.get("system_prompt", ""))
        truncated = text[:60].replace("\n", " ") + "..." if len(text) > 60 else text.replace("\n", " ")
        self._generator_prompt_label.setText(f'"{truncated}"')

    def _on_generator_model_changed(self, index: int) -> None:
        model_id = self._generator_model_combo.itemData(index)
        self._pending_generator_config["model"] = model_id
        self.mark_dirty()

    def _on_edit_generator_prompt(self) -> None:
        config = self._config_service.get_description_generator_config()
        current_prompt = self._pending_generator_config.get("system_prompt", config.get("system_prompt", ""))

        dialog = PromptTemplateDialog(current_prompt=current_prompt, parent=self)
        if dialog.exec():
            result = dialog.get_result()
            if result is not None:
                self._pending_generator_config["system_prompt"] = result
                self._update_generator_prompt_label()
                self.mark_dirty()

    def _load_prompts(self):
        settings_data = self._config_service.get_settings_data()
        prompts = settings_data.get("prompts", [])
        self._prompt_list.set_prompts(prompts)
        self._update_button_states()

    def _on_prompt_selected(self, prompt_data: dict):
        self._save_current_editor_state()
        prompt_id = prompt_data.get("id", "")
        self._prompt_editor.load_prompt(prompt_id, prompt_data)
        self._update_button_states()

    def _save_current_editor_state(self):
        result = self._prompt_editor.get_prompt_data()
        if not result:
            return
        prompt_id, data = result
        if self._prompt_editor.is_new_prompt():
            self._config_service.add_prompt(data, persist=False)
            self._prompt_editor.mark_saved()
        else:
            self._config_service.update_prompt(prompt_id, data, persist=False)

    def _on_add_prompt(self):
        self._prompt_list.clear_selection()
        self._prompt_editor.set_new_mode()
        self._update_button_states()
        self.mark_dirty()

    def _on_delete_prompt(self):
        current = self._prompt_list.current_item()
        if current:
            prompt_id = current.data(Qt.UserRole)
            self._config_service.delete_prompt(prompt_id, persist=False)
            self._prompt_editor.clear()
            self._load_prompts()
            self.mark_dirty()

    def _on_order_changed(self, prompt_ids: list):
        self._config_service.update_prompts_order(prompt_ids, persist=False)
        self.mark_dirty()

    def _update_button_states(self):
        has_selection = self._prompt_list.current_item() is not None
        current_row = self._prompt_list.current_row()
        count = self._prompt_list.count()

        self._delete_btn.setEnabled(has_selection and not self._prompt_editor.is_new_prompt())
        self._up_btn.setEnabled(has_selection and current_row > 0)
        self._down_btn.setEnabled(has_selection and current_row < count - 1)

    def save_changes(self) -> bool:
        result = self._prompt_editor.get_prompt_data()
        if result:
            prompt_id, data = result
            if self._prompt_editor.is_new_prompt():
                self._config_service.add_prompt(data, persist=False)
                self._prompt_editor.mark_saved()
            else:
                self._config_service.update_prompt(prompt_id, data, persist=False)
            self._load_prompts()
            self._prompt_list.select_by_id(prompt_id)

        if self._pending_generator_config:
            self._config_service.update_description_generator_config(self._pending_generator_config, persist=False)
            self._pending_generator_config = {}

        self.mark_clean()
        return True

    def load_settings(self) -> None:
        self._prompt_editor.clear()
        self._load_prompts()
        self._load_generator_config()
        self._pending_generator_config = {}
        self.mark_clean()
