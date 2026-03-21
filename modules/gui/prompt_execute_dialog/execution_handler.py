"""Execution handler for PromptExecuteDialog."""

import contextlib
import time
from typing import TYPE_CHECKING

from PySide6.QtCore import QTimer
from PySide6.QtGui import QTextCursor

from core.models import ExecutionResult, MenuItem
from modules.gui.icons import create_icon
from modules.gui.prompt_execute_dialog.data import (
    OutputVersionState,
    create_node,
)
from modules.gui.shared.theme import get_text_edit_content_height
from modules.gui.shared.widgets import BUBBLE_TEXT_EDIT_MIN_HEIGHT

if TYPE_CHECKING:
    from modules.gui.prompt_execute_dialog.dialog import PromptExecuteDialog


class ExecutionHandler:
    """Handles prompt execution, streaming, and result processing.

    Manages the execution lifecycle including:
    - Signal connections to prompt store service
    - Streaming chunk processing with throttling
    - Execution result handling
    - Button state management (send/stop toggle)
    - Tab isolation for parallel executions
    """

    def __init__(self, dialog: "PromptExecuteDialog"):
        self.dialog = dialog

        # Tab association (for streaming isolation)
        self._tab_id: str | None = None

        # Execution state
        self._current_execution_id: str | None = None
        self._waiting_for_result = False
        self._stop_button_active: str | None = None  # "alt" or "ctrl"

        # Streaming state
        self._is_streaming = False
        self._streaming_accumulated = ""
        self._last_ui_update_time = 0

        # Signal connection tracking
        self._execution_signal_connected = False
        self._streaming_signal_connected = False

        # Streaming throttle timer (60fps max)
        self._streaming_throttle_timer = QTimer()
        self._streaming_throttle_timer.setSingleShot(True)
        self._streaming_throttle_timer.setInterval(16)
        self._streaming_throttle_timer.timeout.connect(self._flush_streaming_update)

        # Tree-based execution tracking
        self._pending_user_node_id: str | None = None
        self._pending_assistant_node_id: str | None = None

        # Close-after-result tracking (for Ctrl+Enter flow)
        self._close_after_result = False

        # Pending result for inactive tabs (result received while tab was not active)
        self._pending_result: ExecutionResult | None = None

    @property
    def is_waiting(self) -> bool:
        """Check if waiting for execution result."""
        return self._waiting_for_result

    @property
    def is_streaming(self) -> bool:
        """Check if currently streaming."""
        return self._is_streaming

    @property
    def current_execution_id(self) -> str | None:
        """Get current execution ID."""
        return self._current_execution_id

    def _is_tab_active(self) -> bool:
        """Check if this handler's tab is currently active."""
        if not self._tab_id:
            return True  # No tab system, always active
        return self.dialog._active_tab_id == self._tab_id

    def _get_prompt_store_service(self):
        """Get the prompt store service."""
        return self.dialog._prompt_store_service

    # --- Signal Management ---

    def connect_execution_signal(self):
        """Connect to execution completed signal."""
        if self._execution_signal_connected:
            return
        service = self._get_prompt_store_service()
        if service and hasattr(service, "_menu_coordinator"):
            try:
                service._menu_coordinator.execution_completed.connect(self.on_execution_result)
                self._execution_signal_connected = True
            except Exception:
                pass

    def disconnect_execution_signal(self):
        """Disconnect from execution completed signal."""
        if not self._execution_signal_connected:
            return
        service = self._get_prompt_store_service()
        if service and hasattr(service, "_menu_coordinator"):
            with contextlib.suppress(Exception):
                service._menu_coordinator.execution_completed.disconnect(self.on_execution_result)
        self._execution_signal_connected = False

    def connect_streaming_signal(self):
        """Connect to streaming chunk signal for live updates."""
        if self._streaming_signal_connected:
            return
        service = self._get_prompt_store_service()
        if service and hasattr(service, "_menu_coordinator"):
            try:
                service._menu_coordinator.streaming_chunk.connect(self.on_streaming_chunk)
                self._streaming_signal_connected = True
            except Exception:
                pass

    def disconnect_streaming_signal(self):
        """Disconnect from streaming chunk signal."""
        if not self._streaming_signal_connected:
            return
        service = self._get_prompt_store_service()
        if service and hasattr(service, "_menu_coordinator"):
            with contextlib.suppress(Exception):
                service._menu_coordinator.streaming_chunk.disconnect(self.on_streaming_chunk)
        self._streaming_signal_connected = False

    def disconnect_all_signals(self):
        """Disconnect all signals."""
        self.disconnect_execution_signal()
        self.disconnect_streaming_signal()

    # --- Streaming ---

    def on_streaming_chunk(self, chunk: str, accumulated: str, is_final: bool, execution_id: str = ""):
        """Handle streaming chunk with adaptive throttling."""
        # Filter by execution_id FIRST - this is the primary discriminator
        if execution_id and self._current_execution_id:
            if execution_id != self._current_execution_id:
                return  # Not for this handler

        # THEN check handler state
        if not self._waiting_for_result:
            return

        if not self._is_streaming and not is_final:
            self._is_streaming = True
            self._streaming_accumulated = ""

        self._streaming_accumulated = accumulated

        if is_final:
            self._flush_streaming_update()
            self._is_streaming = False
            self._streaming_throttle_timer.stop()
            return

        # Adaptive throttling
        current_time = time.time() * 1000
        time_since_update = current_time - self._last_ui_update_time

        # Small chunks or enough time passed - update immediately
        if len(chunk) < 10 or time_since_update >= 16:
            self._flush_streaming_update()
        elif not self._streaming_throttle_timer.isActive():
            self._streaming_throttle_timer.start()

    def _flush_streaming_update(self):
        """Update UI with accumulated streaming text."""
        if not self._streaming_accumulated:
            return

        self._last_ui_update_time = time.time() * 1000

        # If tab is NOT active, only keep accumulated content (don't touch shared state).
        # Content is preserved in _streaming_accumulated for when tab becomes active.
        if not self._is_tab_active():
            return

        dialog = self.dialog

        # Update tree node content during streaming (in case of rebuild)
        if dialog._conversation_tree and self._pending_assistant_node_id:
            node = dialog._conversation_tree.get_node(self._pending_assistant_node_id)
            if node:
                node.content = self._streaming_accumulated

        # Get correct output text edit based on turn number
        output_edit = self._get_current_output_edit()

        # Update text without triggering undo stack
        output_edit.blockSignals(True)
        output_edit.setPlainText(self._streaming_accumulated)
        cursor = output_edit.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        output_edit.setTextCursor(cursor)
        output_edit.blockSignals(False)

        # Update height for tree-based bubbles in expanded mode during streaming
        if self._pending_assistant_node_id:
            for bubble in reversed(dialog._message_bubbles):
                if hasattr(bubble, "node_id") and bubble.node_id == self._pending_assistant_node_id:
                    if hasattr(bubble, "header") and not bubble.header.is_wrapped():
                        content_height = get_text_edit_content_height(
                            bubble.text_edit, min_height=BUBBLE_TEXT_EDIT_MIN_HEIGHT
                        )
                        bubble.text_edit.setMinimumHeight(content_height)
                        bubble.text_edit.setMaximumHeight(content_height)
                    break

        # Auto-scroll to show new streaming content
        self.dialog._scroll_to_bottom()

    def _get_current_output_edit(self):
        """Get the current output text edit based on turn number."""
        dialog = self.dialog

        # When using tree-based bubbles, return the assistant bubble's text_edit
        if dialog._conversation_tree and not dialog._conversation_tree.is_empty():
            # First check if there's a pending assistant node we're streaming to
            if self._pending_assistant_node_id:
                for bubble in reversed(dialog._message_bubbles):
                    if hasattr(bubble, "node_id") and bubble.node_id == self._pending_assistant_node_id:
                        return bubble.text_edit

            # Fallback to any assistant bubble
            for bubble in reversed(dialog._message_bubbles):
                if hasattr(bubble, "node_id"):
                    node = dialog._conversation_tree.get_node(bubble.node_id)
                    if node and node.role == "assistant":
                        return bubble.text_edit

        # Legacy fallback
        if dialog._current_turn_number == 1 or not dialog._output_sections:
            return dialog.output_edit
        return dialog._output_sections[-1].text_edit

    # --- Execution ---

    def execute_with_message(
        self,
        message: str,
        keep_open: bool = False,
        regenerate: bool = False,
    ):
        """Execute the prompt with conversation history.

        Uses working context (images + text) from dialog, NOT from persistent storage.
        Context is sent with the prompt but NOT saved to context_manager.

        Args:
            message: The message to use as input (ignored if dynamic sections exist)
            keep_open: If True, keep dialog open and show result
            regenerate: If True, reuse existing output section instead of creating new one
        """
        dialog = self.dialog

        # Get current input from reply section if exists, otherwise original input
        # When using tree-based conversation, ALWAYS use the message parameter
        # (ignore legacy _dynamic_sections to avoid interference)
        if dialog._conversation_tree and not dialog._conversation_tree.is_empty():
            msg_text = message
            msg_images = list(dialog._message_images)
        elif dialog._dynamic_sections:
            section = dialog._dynamic_sections[-1]
            msg_text = section.text_edit.toPlainText()
            msg_images = list(section.turn_images)
        else:
            msg_text = message
            msg_images = list(dialog._message_images)

        # Validate message has content
        if not msg_text.strip() and not msg_images:
            return

        # Clear input after capturing message
        if dialog._dynamic_sections:
            section.text_edit.setPlainText("")
            section.turn_images.clear()
        else:
            dialog.input_edit.setPlainText("")
            dialog._message_images.clear()
            dialog._rebuild_message_image_chips()

        # Get the prompt store service
        service = self._get_prompt_store_service()
        if not service:
            if keep_open:
                dialog._expand_output_section()
                dialog.output_edit.setPlainText("Error: Prompt service not available")
            return

        # Record turn in conversation history
        if dialog._current_turn_number == 0:
            dialog._current_turn_number = 1

        from modules.gui.prompt_execute_dialog.data import ConversationTurn

        turn = ConversationTurn(
            turn_number=dialog._current_turn_number,
            message_text=msg_text,
            message_images=msg_images,
        )

        # Restore version history for regeneration
        if hasattr(dialog, "_pending_version_history"):
            turn.output_versions = dialog._pending_version_history
            delattr(dialog, "_pending_version_history")
        if hasattr(dialog, "_pending_version_undo_states"):
            turn.version_undo_states = dialog._pending_version_undo_states
            delattr(dialog, "_pending_version_undo_states")
        if hasattr(dialog, "_pending_version_index"):
            turn.current_version_index = dialog._pending_version_index
            delattr(dialog, "_pending_version_index")

        dialog._conversation_turns.append(turn)

        # Sync any user edits in bubbles back to tree nodes BEFORE adding new nodes
        dialog._sync_bubbles_to_tree()

        # Add to conversation tree (skip if regeneration already set up nodes)
        if not self._pending_assistant_node_id:
            if dialog._conversation_tree is None:
                from modules.gui.prompt_execute_dialog.data import ConversationTree

                dialog._conversation_tree = ConversationTree()

            # Create user node
            user_node = create_node(
                role="user",
                content=msg_text,
                parent_id=dialog._conversation_tree.current_path[-1] if dialog._conversation_tree.current_path else None,
                images=msg_images,
            )
            dialog._conversation_tree.append_to_current_path(user_node)
            self._pending_user_node_id = user_node.node_id

            # Create placeholder assistant node
            assistant_node = create_node(
                role="assistant",
                content="",
                parent_id=user_node.node_id,
            )
            dialog._conversation_tree.append_to_current_path(assistant_node)
            self._pending_assistant_node_id = assistant_node.node_id

            # Rebuild bubbles immediately so user sees message + placeholder output
            dialog._rebuild_message_bubbles_from_tree()

        # Build conversation data for API (can use tree or legacy)
        if dialog._conversation_tree and not dialog._conversation_tree.is_empty():
            conv_data = self._build_conversation_data_from_tree()
        else:
            conv_data = self._build_conversation_data()

        # Enable streaming for "Send & Show" mode
        if keep_open:
            conv_data["use_streaming"] = True

        # For backward compatibility, also build full_message for single-turn case
        working_context_text = dialog.context_text_edit.toPlainText().strip()
        full_message = msg_text
        if len(dialog._conversation_turns) == 1 and working_context_text:
            full_message = f"<context>\n{working_context_text}\n</context>\n\n{msg_text}"

        # Create a modified menu item with conversation data
        modified_item = MenuItem(
            id=dialog.menu_item.id,
            label=dialog.menu_item.label,
            item_type=dialog.menu_item.item_type,
            action=dialog.menu_item.action,
            data={
                **(dialog.menu_item.data or {}),
                "custom_context": full_message,
                "conversation_data": conv_data,
                "skip_clipboard_copy": keep_open,
                "is_from_dialog": True,
            },
            enabled=dialog.menu_item.enabled,
        )

        # Always connect execution signal to save history
        self._waiting_for_result = True
        self.connect_execution_signal()
        self._close_after_result = not keep_open

        if keep_open:
            # Connect streaming for live updates
            self.connect_streaming_signal()

            status_text = "Regenerating..." if regenerate else "Executing..."

            # Create output section for this turn
            self._setup_output_section_for_execution(regenerate, status_text)

            # Transform button to stop mode and disable the other button
            self._transform_button_to_stop(is_alt_enter=True)
            dialog.send_copy_btn.setEnabled(False)

        # Execute using the prompt execution handler and capture execution_id
        for handler in service.execution_service.handlers:
            if handler.can_handle(modified_item):
                if hasattr(handler, "async_manager"):
                    # Always use async execution so it's cancellable via context menu
                    execution_id = handler.async_manager.execute_prompt_async(modified_item, full_message)

                    # Track execution for result handling
                    self._current_execution_id = execution_id

                    if not keep_open:
                        # Hide dialog immediately for quick visual feedback
                        # Dialog will close after history save in on_execution_result
                        dialog.hide()
                else:
                    # Fallback for handlers without async_manager
                    handler.execute(modified_item, full_message)
                    if not keep_open:
                        # No async execution, close immediately (no history save possible)
                        self._close_after_result = False
                        self._waiting_for_result = False
                        dialog.accept()
                return

        # Fallback: use execution callback
        if dialog.execution_callback:
            if dialog.menu_item.data:
                dialog.menu_item.data["custom_context"] = full_message
            dialog.execution_callback(dialog.menu_item, False)
            if not keep_open:
                # Callback execution, close immediately (no history save possible)
                self._close_after_result = False
                self._waiting_for_result = False
                dialog.accept()

    def _setup_output_section_for_execution(self, regenerate: bool, status_text: str):
        """Set up output section before execution starts."""
        dialog = self.dialog

        # Skip legacy output section creation when using tree-based bubbles
        if dialog._conversation_tree and not dialog._conversation_tree.is_empty():
            dialog._scroll_to_bottom()
            return

        if dialog._current_turn_number == 1:
            # First turn uses existing output section
            dialog._expand_output_section()
            dialog.output_edit.setPlainText(status_text)
            # Set expanded mode and update height after text is set
            dialog.output_header.set_wrap_state(False)
            content_height = get_text_edit_content_height(dialog.output_edit)
            dialog.output_edit.setMinimumHeight(content_height)
        elif regenerate and dialog._output_sections:
            # Regenerating - reuse existing output section
            output_section = dialog._output_sections[-1]
            output_section.text_edit.blockSignals(True)
            output_section.text_edit.setPlainText(status_text)
            output_section.text_edit.blockSignals(False)
            # Set expanded mode
            output_section.header.set_wrap_state(False)
            content_height = get_text_edit_content_height(output_section.text_edit)
            output_section.text_edit.setMinimumHeight(content_height)
        else:
            # Subsequent turns create new output section
            output_section = dialog._create_dynamic_output_section(dialog._current_turn_number)
            dialog._output_sections.append(output_section)
            dialog.sections_layout.addWidget(output_section)
            output_section.text_edit.installEventFilter(dialog)
            output_section.text_edit.setPlainText(status_text)
            # Set expanded mode and update height after text is set
            output_section.header.set_wrap_state(False)
            content_height = get_text_edit_content_height(output_section.text_edit)
            output_section.text_edit.setMinimumHeight(content_height)
            dialog._renumber_sections()
            dialog._update_delete_button_visibility()
            dialog._scroll_to_bottom()

    def _build_conversation_data(self) -> dict:
        """Build conversation history for API."""
        dialog = self.dialog
        context_text = dialog.context_text_edit.toPlainText().strip()
        context_images = [
            {"data": img.data, "media_type": img.media_type or "image/png"} for img in dialog._current_images
        ]

        turns = []
        for i, turn in enumerate(dialog._conversation_turns):
            turn_data = {
                "role": "user",
                "text": turn.message_text,
                "images": [
                    {"data": img.data, "media_type": img.media_type or "image/png"} for img in turn.message_images
                ],
            }
            # First turn includes context
            if i == 0:
                turn_data["context_text"] = context_text
                turn_data["context_images"] = context_images

            turns.append(turn_data)

            if turn.is_complete and turn.output_versions:
                # Use the currently selected version
                selected_text = turn.output_versions[turn.current_version_index]
                turns.append({"role": "assistant", "text": selected_text})
            elif turn.is_complete and turn.output_text:
                # Fallback for backward compatibility
                turns.append({"role": "assistant", "text": turn.output_text})

        return {"turns": turns}

    def _build_conversation_data_from_tree(self) -> dict:
        """Build conversation history for API from tree structure."""
        dialog = self.dialog
        tree = dialog._conversation_tree
        context_text = dialog.context_text_edit.toPlainText().strip()
        context_images = [
            {"data": img.data, "media_type": img.media_type or "image/png"} for img in dialog._current_images
        ]

        turns = []
        if not tree or tree.is_empty():
            return {"turns": turns}

        pairs = tree.get_message_pairs()
        for i, (user_node, assistant_node) in enumerate(pairs):
            turn_data = {
                "role": "user",
                "text": user_node.content,
                "images": [
                    {"data": img.data, "media_type": img.media_type or "image/png"} for img in user_node.images
                ],
            }
            if i == 0:
                turn_data["context_text"] = context_text
                turn_data["context_images"] = context_images

            turns.append(turn_data)

            if assistant_node and assistant_node.content:
                turns.append({"role": "assistant", "text": assistant_node.content})

        return {"turns": turns}

    def _clear_regeneration_flag(self) -> bool:
        """Clear regeneration flag and return whether it was set."""
        dialog = self.dialog
        is_regeneration = getattr(dialog, "_pending_is_regeneration", False)
        if hasattr(dialog, "_pending_is_regeneration"):
            delattr(dialog, "_pending_is_regeneration")
        return is_regeneration

    def _update_turn_with_output(self, output_text: str, is_regeneration: bool):
        """Update turn's version history and mark as complete."""
        dialog = self.dialog
        if not dialog._conversation_turns:
            return

        turn = dialog._conversation_turns[-1]
        turn.output_text = output_text
        turn.is_complete = True

        if not output_text:
            return

        if is_regeneration and turn.output_versions:
            turn.output_versions[turn.current_version_index] = output_text
            turn.version_undo_states[turn.current_version_index] = OutputVersionState(
                undo_stack=[], redo_stack=[], last_text=output_text
            )
        else:
            turn.output_versions.append(output_text)
            turn.current_version_index = len(turn.output_versions) - 1
            turn.version_undo_states.append(OutputVersionState(undo_stack=[], redo_stack=[], last_text=output_text))

    def _update_tree_with_output(self, output_text: str):
        """Update tree node with execution output."""
        dialog = self.dialog
        tree = dialog._conversation_tree
        if not tree or not self._pending_assistant_node_id:
            return

        node = tree.get_node(self._pending_assistant_node_id)
        if node:
            node.content = output_text or ""
            node.last_text = output_text or ""

        # Clear pending node IDs
        self._pending_user_node_id = None
        self._pending_assistant_node_id = None

    def _update_version_ui(self, output_text: str):
        """Update version display and sync undo state in UI."""
        dialog = self.dialog
        if not dialog._conversation_turns:
            return

        turn = dialog._conversation_turns[-1]

        if turn.turn_number == 1 or not dialog._output_sections:
            dialog.output_header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))
            if turn.output_versions:
                dialog._output_undo_stack.clear()
                dialog._output_redo_stack.clear()
                dialog._last_output_text = output_text or ""
                dialog._update_undo_redo_buttons()
        else:
            section = dialog._output_sections[-1]
            section.header.set_version_info(turn.current_version_index + 1, len(turn.output_versions))
            if turn.output_versions:
                section.undo_stack.clear()
                section.redo_stack.clear()
                section.last_text = output_text or ""
                dialog._update_dynamic_section_buttons(section)

    def _finalize_execution_ui(self):
        """Update send buttons after execution ends."""
        self.dialog._update_send_buttons_state()

    def stop_execution(self):
        """Cancel this dialog's execution only."""
        if not self._current_execution_id:
            return

        execution_id_to_cancel = self._current_execution_id

        is_regeneration = self._clear_regeneration_flag()
        should_close = self._close_after_result

        self._waiting_for_result = False
        self._current_execution_id = None
        self._close_after_result = False
        self._revert_button_to_send_state()

        output_edit = self._get_current_output_edit()
        current_text = output_edit.toPlainText()
        if current_text and current_text not in ("Executing...", "Regenerating..."):
            cancelled_text = current_text + "\n\n[cancelled]"
        else:
            cancelled_text = "[cancelled]"
        output_edit.setPlainText(cancelled_text)

        self._update_turn_with_output(cancelled_text, is_regeneration)
        self._update_version_ui(cancelled_text)
        self._finalize_execution_ui()

        service = self._get_prompt_store_service()
        if service:
            service.execution_service.cancel_execution(execution_id_to_cancel, silent=True)

        # Close dialog if it was hidden (Ctrl+Enter flow)
        if should_close:
            self.dialog.accept()

    # --- Result Handling ---

    def on_execution_result(self, result: ExecutionResult, execution_id: str = ""):
        """Handle execution result for multi-turn conversation."""
        if execution_id and self._current_execution_id:
            if execution_id != self._current_execution_id:
                return

        if not self._waiting_for_result:
            return

        # If tab is NOT active, store result and defer processing
        if not self._is_tab_active():
            self._pending_result = result
            self._waiting_for_result = False
            self.disconnect_execution_signal()
            self.disconnect_streaming_signal()
            return

        # Tab is active - process now
        self._waiting_for_result = False
        self._current_execution_id = None
        self.disconnect_execution_signal()
        self.disconnect_streaming_signal()
        self._revert_button_to_send_state()

        self._finalize_result(result, is_pending=False)

        if self._close_after_result:
            self._close_after_result = False
            self.dialog.accept()

    def _process_pending_result(self):
        """Process a result that was received while tab was inactive."""
        if not self._pending_result:
            return

        result = self._pending_result
        self._pending_result = None
        self._current_execution_id = None
        self._revert_button_to_send_state()

        self._finalize_result(result, is_pending=True)

    def _finalize_result(self, result: ExecutionResult, is_pending: bool = False):
        """Finalize execution result - common logic for active and pending results."""
        dialog = self.dialog
        is_regeneration = self._clear_regeneration_flag()
        is_streaming = result.metadata and result.metadata.get("streaming", False)

        # Get output text
        if is_streaming and self._streaming_accumulated:
            output_text = self._streaming_accumulated
        else:
            output_text = result.content if result.success else None

        # Update state
        self._update_turn_with_output(output_text, is_regeneration)
        self._update_tree_with_output(output_text)
        dialog._sync_bubbles_to_tree()
        self._update_version_ui(output_text)
        self._finalize_execution_ui()

        # Rebuild bubbles if needed
        # - For pending results: always rebuild (restored from old state)
        # - For active results: only for non-streaming
        should_rebuild = is_pending or not is_streaming
        if should_rebuild and dialog._conversation_tree and not dialog._conversation_tree.is_empty():
            dialog._rebuild_message_bubbles_from_tree()

        # Update output text widget
        output_edit = self._get_current_output_edit()
        if is_pending or not is_streaming or not result.success:
            if result.success and output_text:
                output_edit.setPlainText(output_text)
            elif result.error:
                output_edit.setPlainText(f"Error: {result.error}")
            else:
                output_edit.setPlainText("No output received")

        # Adjust height for legacy sections
        if not is_pending:
            if dialog._current_turn_number == 1 or not dialog._output_sections:
                if not dialog.output_header.is_wrapped():
                    content_height = get_text_edit_content_height(dialog.output_edit)
                    dialog.output_edit.setMinimumHeight(content_height)
            else:
                section = dialog._output_sections[-1]
                if not section.header.is_wrapped():
                    content_height = get_text_edit_content_height(section.text_edit)
                    section.text_edit.setMinimumHeight(content_height)

        dialog._scroll_to_bottom()

        if result.success:
            dialog._save_to_history()

    # --- Button State Management ---

    def _transform_button_to_stop(self, is_alt_enter: bool):
        """Transform send button to stop button during execution."""
        dialog = self.dialog

        if is_alt_enter:
            dialog.send_show_btn.setIcon(create_icon("square", "#f0f0f0", 16))
            dialog.send_show_btn.setToolTip("Stop execution (Enter)")
            with contextlib.suppress(TypeError):
                dialog.send_show_btn.clicked.disconnect()
            dialog.send_show_btn.clicked.connect(self._on_stop_button_clicked)
            dialog.send_show_btn.setEnabled(True)
            self._stop_button_active = "alt"
        else:
            dialog.send_copy_btn.setIcon(create_icon("square", "#f0f0f0", 16))
            dialog.send_copy_btn.setToolTip("Stop execution (Ctrl+Enter)")
            with contextlib.suppress(TypeError):
                dialog.send_copy_btn.clicked.disconnect()
            dialog.send_copy_btn.clicked.connect(self._on_stop_button_clicked)
            dialog.send_copy_btn.setEnabled(True)
            self._stop_button_active = "ctrl"

    def _on_stop_button_clicked(self):
        """Handle stop button click."""
        self.stop_execution()

    def _revert_button_to_send_state(self):
        """Revert stop button back to send button.

        Only updates the actual button widgets if this handler's tab is active.
        Always clears the handler's internal stop_button_active state.
        """
        dialog = self.dialog

        # Only touch the actual buttons if this tab is active
        if self._is_tab_active():
            if self._stop_button_active == "alt":
                with contextlib.suppress(TypeError):
                    dialog.send_show_btn.clicked.disconnect()
                dialog.send_show_btn.clicked.connect(dialog._on_send_show)
                dialog.send_show_btn.setIcon(create_icon("send-horizontal", "#444444", 16))
                dialog.send_show_btn.setToolTip("Send & Show Result (Enter)")
            elif self._stop_button_active == "ctrl":
                with contextlib.suppress(TypeError):
                    dialog.send_copy_btn.clicked.disconnect()
                dialog.send_copy_btn.clicked.connect(dialog._on_send_copy)
                dialog.send_copy_btn.setIcon(create_icon("copy", "#444444", 16))
                dialog.send_copy_btn.setToolTip("Send & Copy to Clipboard (Ctrl+Enter)")
            dialog._update_send_buttons_state()

        # Always clear the handler's internal state
        self._stop_button_active = None

    def cleanup(self):
        """Clean up handler on dialog close."""
        self.disconnect_all_signals()
        if self._is_streaming:
            self._streaming_throttle_timer.stop()
            self._is_streaming = False
