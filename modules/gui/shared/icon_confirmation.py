"""Reusable icon confirmation effect for IconButton widgets."""

from PySide6.QtCore import QTimer


def flash_confirmation(button, confirm_icon: str = "check", duration_ms: int = 2000):
    original_icon = button._icon_name
    button.set_icon(confirm_icon)
    button.setEnabled(False)
    QTimer.singleShot(duration_ms, lambda: _restore(button, original_icon))


def _restore(button, original_icon: str):
    button.set_icon(original_icon)
    button.setEnabled(True)
