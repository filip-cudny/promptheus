import { emit } from "@tauri-apps/api/event";
import type { Fixture } from "../mocks";
import { chatTitle } from "./conversation";
import { appSettings } from "./settings";

export const label = "palette";

function tab(id: string, title: string, provider_id: string | null, recency: number) {
  return {
    id,
    kind: provider_id ? "provider" : "native",
    provider_id,
    native_tab_id: provider_id ? null : `native-${id}`,
    title,
    live: true,
    streaming: false,
    recency,
  };
}

const tabs = [
  tab("tab-1", chatTitle, null, 4),
  tab("tab-2", "Weekend trip ideas for Lisbon", "chatgpt", 3),
  tab("tab-3", "Quarterly report outline", "claude", 2),
];

export const fixture: Fixture = {
  get_settings: appSettings,
};

export async function show() {
  await emit("palette:show", {
    host_label: "conversation-dialog",
    active_id: "promptheus",
    providers: appSettings.webview_providers,
    tabs: { tabs, active_id: "tab-1" },
    current_title: chatTitle,
  });
}
