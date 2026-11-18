import type { Fixture } from "../mocks";
import { fixture as conversationFixture } from "./conversation";
import { appSettings } from "./settings";

export { label } from "./conversation";

const transcript = "Why does the cart total fail? The screenshot shows the error.";

const answer = [
  "`reduce` has no starting value, so for an empty cart it has nothing to return and `total` stays `undefined`. Pass `0` as the second argument:",
  "",
  "```js",
  "function cartTotal(items) {",
  "  return items.reduce((sum, item) => sum + item.price, 0);",
  "}",
  "```",
  "",
  "An empty cart now totals `0` instead of raising the error.",
].join("\n");

const CHUNK_SIZE = 8;
const CHUNK_DELAY_MS = 85;

const chunks = Array.from({ length: Math.ceil(answer.length / CHUNK_SIZE) }, (_, i) =>
  answer.slice(i * CHUNK_SIZE, (i + 1) * CHUNK_SIZE),
);

interface Gate {
  promise: Promise<void>;
  open: () => void;
}

function gate(): Gate {
  let open!: () => void;
  const promise = new Promise<void>((resolve) => (open = resolve));
  return { promise, open };
}

const chatModel = { display_name: "GPT-6", model: "gpt-6" };
const settings = {
  ...appSettings,
  models: appSettings.models.map((m) => (m.id === appSettings.surfaces.chat.generation.model_id ? { ...m, ...chatModel } : m)),
};

const chatOpened = gate();
const answerStarted = gate();

export const globals = { openChat: chatOpened.open, startAnswer: answerStarted.open };

function drawErrorScreenshot(): string {
  const canvas = document.createElement("canvas");
  canvas.width = 360;
  canvas.height = 150;
  const g = canvas.getContext("2d")!;
  g.fillStyle = "#1f2330";
  g.fillRect(0, 0, canvas.width, canvas.height);
  g.font = "13px monospace";
  g.fillStyle = "#c3cad9";
  g.fillText("function cartTotal(items) {", 14, 28);
  g.fillText("  return items.reduce(", 14, 48);
  g.fillText("    (sum, item) => sum + item.price", 14, 68);
  g.fillText("  );", 14, 88);
  g.fillText("}", 14, 108);
  g.fillStyle = "#5b2230";
  g.fillRect(0, 118, canvas.width, 32);
  g.fillStyle = "#ff9aa8";
  g.fillText("CartError: total is undefined", 14, 138);
  return canvas.toDataURL("image/png").split(",")[1];
}

const contextImage = { item_type: "image", number: 1, data: drawErrorScreenshot(), media_type: "image/png" };

type CallbackRunner = { runCallback(id: number, data: unknown): void };

async function streamAnswer(channel: { id: number }) {
  const { runCallback } = (window as unknown as { __TAURI_INTERNALS__: CallbackRunner }).__TAURI_INTERNALS__;
  let index = 0;
  const send = (message: unknown) => runCallback(channel.id, { index: index++, message });
  await answerStarted.promise;
  let accumulated = "";
  for (const delta of chunks) {
    await new Promise((resolve) => setTimeout(resolve, CHUNK_DELAY_MS));
    accumulated += delta;
    send({ event: "chunk", data: { delta, accumulated, thinking_delta: null, accumulated_thinking: null } });
  }
  send({ event: "done", data: { full_text: answer, full_thinking: null, prompt_tokens: 236, completion_tokens: 71 } });
}

export const fixture: Fixture = {
  ...conversationFixture,
  get_settings: settings,
  count_conversation_tokens: 312,
  get_dialog_init_params: async () => {
    await chatOpened.promise;
    return {
      skill_id: "",
      skill_name: "",
      skill_model: null,
      history_entry_id: null,
      last_interaction_only: false,
      initial_input: transcript,
      auto_send_input: true,
      new_chat: false,
      invocation_source: "pill",
      copy_token: "answer-copy-1",
    };
  },
  has_context: true,
  get_context_items: [contextImage],
  get_labelled_context: null,
  execute_conversation_from_tree: (args) => streamAnswer(args.onEvent as { id: number }),
  copy_chat_answer_if_unchanged: true,
  resolve_skill_input: { had_skills: false, applied_skills: [] },
  generate_conversation_title: "Empty cart total error",
  add_conversation_entry: "entry-dictation-1",
  update_conversation_entry: null,
  update_history_entry_title: null,
};
