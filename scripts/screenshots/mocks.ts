import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import * as settings from "./views/settings";
import * as contextMenu from "./views/context-menu";
import * as conversation from "./views/conversation";
import * as palette from "./views/palette";
import * as history from "./views/history";
import * as setup from "./views/setup";
import * as recordingPill from "./views/recording-pill";
import * as quickActionMenu from "./views/quick-action-menu";
import * as notification from "./views/notification";
import * as dictationPill from "./views/dictation-pill";
import * as dictationChat from "./views/dictation-chat";

export type Fixture = Record<string, unknown | ((args: Record<string, unknown>) => unknown)>;

interface View {
  label: string;
  fixture: Fixture;
  globals?: Record<string, unknown>;
  show?: () => Promise<void>;
}

const VIEWS: Record<string, View> = {
  settings,
  "context-menu": contextMenu,
  conversation,
  palette,
  history,
  setup,
  "recording-pill": recordingPill,
  "quick-action-menu": quickActionMenu,
  notification,
  "dictation-pill": dictationPill,
  "dictation-chat": dictationChat,
};

const name = new URLSearchParams(location.search).get("view") ?? "";
const view = VIEWS[name];
if (!view) throw new Error(`screenshot view "${name}" is not registered in mocks.ts`);

mockWindows(view.label);
Object.assign(window, view.globals);
mockIPC(
  async (cmd, args) => {
    if (cmd.startsWith("plugin:log|")) return null;
    if (!Object.hasOwn(view.fixture, cmd)) {
      console.error(`screenshot view "${name}": fixture lacks command "${cmd}"`);
      throw new Error(`command ${cmd} is not in the "${name}" fixture`);
    }
    const answer = view.fixture[cmd];
    return typeof answer === "function" ? answer((args ?? {}) as Record<string, unknown>) : answer;
  },
  { shouldMockEvents: true },
);

(window as unknown as { __screenshot: unknown }).__screenshot = {
  envKeys: Object.keys(import.meta.env),
  show: view.show,
};
