import type { Animation, Stage, Step } from "../host";

const RECORDING_START_MS = 1000;
const LEVEL_TICK_MS = 120;
const POINTER_TRAVEL_MS = 700;

export const backdrop = `
<style>
  .desktop { position: absolute; inset: 0; background: linear-gradient(135deg, #3d4b63, #232a38); }
  .editor { position: absolute; left: 40px; top: 20px; width: 720px; height: 400px; background: #1f2330; border-radius: 10px; box-shadow: 0 18px 50px rgba(0, 0, 0, 0.45); overflow: hidden; color: #c3cad9; }
  .editor-bar { height: 34px; background: #171a24; display: flex; align-items: center; justify-content: center; font-size: 13px; color: #8c93a6; }
  .editor-code { margin: 0; padding: 22px 30px; font: 17px/1.65 var(--font-mono, monospace); }
  .editor-code .kw { color: #c792ea; }
  .editor-code .fn { color: #82aaff; }
  .editor-code .num { color: #f78c6c; }
  .editor-code .bad { text-decoration: underline wavy #ff6b81; }
  .editor-error { position: absolute; left: 0; right: 0; bottom: 0; padding: 16px 30px 20px; background: #3a1f2b; color: #ff9aa8; font: 16px/1.6 var(--font-mono, monospace); }
  .chat-shadow { position: absolute; left: 110px; top: 30px; width: 580px; height: 500px; border-radius: 10px; box-shadow: 0 24px 70px rgba(0, 0, 0, 0.6); visibility: hidden; }
  #pointer { position: absolute; z-index: 10; left: 600px; top: 360px; width: 20px; height: 28px; background: #fff; clip-path: polygon(0 0, 0 82%, 28% 64%, 48% 100%, 64% 92%, 45% 58%, 82% 58%); opacity: 0; transition: left ${POINTER_TRAVEL_MS}ms ease-in-out, top ${POINTER_TRAVEL_MS}ms ease-in-out, opacity 200ms, transform 120ms; transform-origin: 0 0; }
  #pointer.pressed { transform: scale(0.8); }
</style>
<div class="desktop"></div>
<div class="editor">
  <div class="editor-bar">cart.js</div>
<pre class="editor-code"><span class="kw">function</span> <span class="fn">cartTotal</span>(items) {
  <span class="kw">return</span> items.<span class="fn">reduce</span>(
    (sum, item) =&gt; <span class="bad">sum + item.price</span>
  );
}

<span class="fn">console</span>.<span class="fn">log</span>(<span class="fn">cartTotal</span>([]));</pre>
  <div class="editor-error">CartError: total is undefined<br />    at cartTotal (cart.js:2)</div>
</div>
<div class="chat-shadow"></div>
<div id="pointer"></div>`;

export const windows: Animation["windows"] = {
  toast: { page: "notification.html", view: "notification", left: 440, top: 24, width: 340, height: 140 },
  pill: { page: "recording-widget.html", view: "dictation-pill", left: 268, top: 436, width: 264, height: 104 },
  chat: { page: "conversation-dialog.html", view: "dictation-chat", left: 110, top: 30, width: 580, height: 500 },
};

type Internals = { invoke(command: string, args: Record<string, unknown>): Promise<unknown> };

function emitIn(stage: Stage, windowName: string, event: string, payload: unknown) {
  const { invoke } = stage.frameWindow(windowName).__TAURI_INTERNALS__ as Internals;
  return invoke("plugin:event|emit", { event, payload });
}

let recording = false;

async function runRecording(stage: Stage) {
  recording = true;
  const startedAt = performance.now();
  while (recording) {
    const elapsed = performance.now() - startedAt;
    const level = 0.5 + 0.4 * Math.abs(Math.sin(elapsed / 230)) * (0.7 + 0.3 * Math.sin(elapsed / 710));
    await emitIn(stage, "pill", "recording-widget-state", {
      state: "recording",
      level,
      elapsedMs: RECORDING_START_MS + elapsed,
      monochromatic: false,
    });
    await stage.sleep(LEVEL_TICK_MS);
  }
}

function stopChatButton(stage: Stage) {
  const pill = stage.frameWindow("pill");
  const button = pill.document.querySelector<HTMLButtonElement>("button.secondary")!;
  const rect = button.getBoundingClientRect();
  const origin = pill.frameElement!.getBoundingClientRect();
  const center = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  return { button, center, onStage: { x: origin.x + center.x, y: origin.y + center.y } };
}

function pointer(stage: Stage) {
  return stage.root.querySelector<HTMLElement>("#pointer")!;
}

export const steps: Step[] = [
  {
    caption: "Ctrl+F3",
    shownAtMs: 700,
    durationMs: 1400,
    actions: [
      {
        atMs: 150,
        run: async (stage) => {
          const toast = stage.frameWindow("toast");
          toast.pendingNotifications = [
            { id: "context-set", level: "success", title: "Context set", message: null, monochromatic: false, action: null },
          ];
          stage.setVisible("toast", true);
          await (toast.drainPending as () => Promise<void>)();
        },
      },
    ],
  },
  {
    caption: "Shift+F1",
    shownAtMs: 1200,
    durationMs: 2100,
    actions: [
      {
        atMs: 0,
        run: async (stage) => {
          await stage.show("pill");
          stage.setVisible("pill", true);
          await runRecording(stage);
        },
      },
      { atMs: 600, run: (stage) => stage.setVisible("toast", false) },
    ],
  },
  {
    caption: null,
    shownAtMs: 1300,
    durationMs: 2200,
    actions: [
      {
        atMs: 0,
        run: (stage) => {
          const { onStage } = stopChatButton(stage);
          const arrow = pointer(stage);
          arrow.style.opacity = "1";
          arrow.style.left = `${onStage.x}px`;
          arrow.style.top = `${onStage.y}px`;
        },
      },
      {
        atMs: 800,
        run: async (stage) => {
          const { button, center } = stopChatButton(stage);
          await emitIn(stage, "pill", "recording-widget-pointer", center);
          pointer(stage).classList.add("pressed");
          await stage.sleep(150);
          button.click();
          recording = false;
          await (stage.frameWindow("pill").lockTarget as () => Promise<void>)();
          await emitIn(stage, "pill", "recording-widget-state", {
            state: "processing",
            level: 0,
            elapsedMs: 0,
            monochromatic: false,
          });
          pointer(stage).classList.remove("pressed");
        },
      },
      {
        atMs: 1900,
        run: (stage) => {
          stage.setVisible("pill", false);
          pointer(stage).style.opacity = "0";
        },
      },
    ],
  },
  {
    caption: null,
    shownAtMs: 900,
    durationMs: 1100,
    actions: [
      {
        atMs: 0,
        run: async (stage) => {
          (stage.frameWindow("chat").openChat as () => void)();
          await stage.sleep(300);
          stage.root.querySelector<HTMLElement>(".chat-shadow")!.style.visibility = "visible";
          stage.setVisible("chat", true);
        },
      },
    ],
  },
  {
    caption: null,
    shownAtMs: 1800,
    durationMs: 3500,
    actions: [{ atMs: 0, run: (stage) => (stage.frameWindow("chat").startAnswer as () => void)() }],
  },
];
