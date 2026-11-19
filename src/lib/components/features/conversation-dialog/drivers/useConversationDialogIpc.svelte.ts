import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onContextChanged } from "$lib/services/events";
import type { InvocationSource } from "$lib/types/history";

export function useConversationDialogIpc(opts: {
  selfTarget: string;
  onRestoreHistory: (entryId: string) => void;
  onVoiceInput: (
    text: string,
    skillId: string,
    skillModel: string | null,
    autoSend: boolean,
    invocationSource: InvocationSource | null,
    copyToken: string | null,
  ) => void;
  onOpenForSkill: (skillId: string, skillName: string, skillModel: string | null) => void;
  onNewConversation: () => void;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string, nextTabId: string | null) => void;
  onActiveProviderChanged: (providerId: string | null) => void;
  onActiveProviderCleared: () => void;
  onMenuReloadActive: () => void;
  onContextChanged: (version: number) => void;
}) {
  let unlistens: UnlistenFn[] = [];

  async function init() {
    const u1 = await listen<{ entry_id: string }>(
      "restore-history",
      (event) => opts.onRestoreHistory(event.payload.entry_id),
      { target: opts.selfTarget },
    );

    const u2 = await listen<{
      text: string;
      skill_id: string;
      skill_model: string | null;
      auto_send: boolean;
      invocation_source: InvocationSource | null;
      copy_token: string | null;
    }>(
      "voice-input",
      (event) =>
        opts.onVoiceInput(
          event.payload.text,
          event.payload.skill_id,
          event.payload.skill_model,
          event.payload.auto_send,
          event.payload.invocation_source,
          event.payload.copy_token,
        ),
      { target: opts.selfTarget },
    );

    const u3 = await listen<{ skill_id: string; skill_name: string; skill_model: string | null }>(
      "open-for-skill",
      (event) =>
        opts.onOpenForSkill(event.payload.skill_id, event.payload.skill_name, event.payload.skill_model),
      { target: opts.selfTarget },
    );

    const u4 = await listen<{ reason: string }>(
      "new-conversation",
      () => opts.onNewConversation(),
      { target: opts.selfTarget },
    );

    const u5 = await listen<{ provider_id: string | null }>(
      "shell:active-changed",
      (event) => {
        opts.onActiveProviderChanged(event.payload.provider_id);
        if (event.payload.provider_id === null) opts.onActiveProviderCleared();
      },
      { target: opts.selfTarget },
    );

    const u6 = await listen(
      "menu:reload-active",
      () => opts.onMenuReloadActive(),
      { target: opts.selfTarget },
    );

    const u7 = await onContextChanged((payload) => opts.onContextChanged(payload.version));

    const u8 = await listen<{ tab_id: string }>(
      "chat:activate-tab",
      (event) => opts.onActivateTab(event.payload.tab_id),
      { target: opts.selfTarget },
    );

    const u9 = await listen<{ tab_id: string; next_tab_id: string | null }>(
      "chat:close-tab",
      (event) => opts.onCloseTab(event.payload.tab_id, event.payload.next_tab_id),
      { target: opts.selfTarget },
    );

    unlistens = [u1, u2, u3, u4, u5, u6, u7, u8, u9];
  }

  function destroy() {
    for (const fn of unlistens) fn();
    unlistens = [];
  }

  return { init, destroy };
}

export type ConversationDialogIpc = ReturnType<typeof useConversationDialogIpc>;
