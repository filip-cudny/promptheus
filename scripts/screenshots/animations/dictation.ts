import type { Animation, Stage, Step } from "../host";

const RECORDING_START_MS = 1000;
const LEVEL_TICK_MS = 120;
const POINTER_TRAVEL_MS = 700;
const PRESS_MS = 150;
const SELECTION = { left: 56, top: 64, right: 470, bottom: 410 };

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
  #capture-overlay { position: absolute; inset: 0; z-index: 5; background-color: rgba(0, 0, 0, 0.4); }
  #capture-frame { position: absolute; box-sizing: border-box; width: 0; height: 0; background-color: rgba(255, 255, 255, 0.15); border: 1px solid white; visibility: hidden; transition: width ${POINTER_TRAVEL_MS}ms ease-in-out, height ${POINTER_TRAVEL_MS}ms ease-in-out; }
  #pointer { position: absolute; z-index: 10; left: 600px; top: 360px; width: 20px; height: 28px; background: #fff; clip-path: polygon(0 0, 0 82%, 28% 64%, 48% 100%, 64% 92%, 45% 58%, 82% 58%); opacity: 0; transition: left ${POINTER_TRAVEL_MS}ms ease-in-out, top ${POINTER_TRAVEL_MS}ms ease-in-out, opacity 200ms, transform 120ms; transform-origin: 0 0; }
  #pointer.pressed { transform: scale(0.8); }
  #pointer.crosshair { width: 23px; height: 23px; margin: -11px 0 0 -11px; clip-path: none; background: linear-gradient(#fff, #fff) center / 1px 100% no-repeat, linear-gradient(#fff, #fff) center / 100% 1px no-repeat; filter: drop-shadow(0 0 1px #000); }
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
<div id="capture-overlay" hidden><div id="capture-frame"></div></div>
<div id="pointer"></div>`;

export const windows: Animation["windows"] = {
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

function pillButton(stage: Stage, selector: string) {
  const pill = stage.frameWindow("pill");
  const button = pill.document.querySelector<HTMLButtonElement>(selector)!;
  const rect = button.getBoundingClientRect();
  const origin = pill.frameElement!.getBoundingClientRect();
  const center = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  return { button, center, onStage: { x: origin.x + center.x, y: origin.y + center.y } };
}

function element(stage: Stage, id: string) {
  return stage.root.querySelector<HTMLElement>(`#${id}`)!;
}

function movePointer(stage: Stage, x: number, y: number) {
  const arrow = element(stage, "pointer");
  arrow.style.opacity = "1";
  arrow.style.left = `${x}px`;
  arrow.style.top = `${y}px`;
}

async function clickPillButton(stage: Stage, selector: string) {
  const { button, center } = pillButton(stage, selector);
  await emitIn(stage, "pill", "recording-widget-pointer", center);
  element(stage, "pointer").classList.add("pressed");
  await stage.sleep(PRESS_MS);
  button.click();
}

const SCREENSHOT_BUTTON = 'button[aria-label="Screenshot"]';
const STOP_CHAT_BUTTON = "button.secondary";

export const steps: Step[] = [
  {
    caption: "Shift+F1",
    shownAtMs: 1000,
    durationMs: 1400,
    actions: [
      {
        atMs: 0,
        run: async (stage) => {
          await stage.show("pill");
          stage.setVisible("pill", true);
          await runRecording(stage);
        },
      },
    ],
  },
  {
    caption: null,
    shownAtMs: 900,
    durationMs: 1450,
    actions: [
      {
        atMs: 0,
        run: (stage) => {
          const { onStage } = pillButton(stage, SCREENSHOT_BUTTON);
          movePointer(stage, onStage.x, onStage.y);
        },
      },
      {
        atMs: 800,
        run: async (stage) => {
          await clickPillButton(stage, SCREENSHOT_BUTTON);
          element(stage, "pointer").classList.remove("pressed");
        },
      },
      {
        atMs: 1250,
        run: async (stage) => {
          await emitIn(stage, "pill", "recording-widget-pointer", null);
          stage.setVisible("pill", false);
        },
      },
    ],
  },
  {
    caption: null,
    shownAtMs: 1700,
    durationMs: 2200,
    actions: [
      {
        atMs: 0,
        run: (stage) => {
          element(stage, "capture-overlay").hidden = false;
          element(stage, "pointer").classList.add("crosshair");
          movePointer(stage, SELECTION.left, SELECTION.top);
        },
      },
      {
        atMs: 800,
        run: (stage) => {
          const frame = element(stage, "capture-frame");
          frame.style.left = `${SELECTION.left}px`;
          frame.style.top = `${SELECTION.top}px`;
          frame.style.visibility = "visible";
        },
      },
      {
        atMs: 850,
        run: (stage) => {
          const frame = element(stage, "capture-frame");
          frame.style.width = `${SELECTION.right - SELECTION.left}px`;
          frame.style.height = `${SELECTION.bottom - SELECTION.top}px`;
          movePointer(stage, SELECTION.right, SELECTION.bottom);
        },
      },
      {
        atMs: 2050,
        run: (stage) => {
          element(stage, "capture-overlay").hidden = true;
          element(stage, "pointer").classList.remove("crosshair");
        },
      },
    ],
  },
  {
    caption: null,
    shownAtMs: 500,
    durationMs: 1000,
    actions: [
      {
        atMs: 100,
        run: async (stage) => {
          await (stage.frameWindow("pill").addImage as () => Promise<void>)();
          stage.setVisible("pill", true);
        },
      },
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
          const { onStage } = pillButton(stage, STOP_CHAT_BUTTON);
          movePointer(stage, onStage.x, onStage.y);
        },
      },
      {
        atMs: 800,
        run: async (stage) => {
          await clickPillButton(stage, STOP_CHAT_BUTTON);
          recording = false;
          await (stage.frameWindow("pill").lockTarget as () => Promise<void>)();
          await emitIn(stage, "pill", "recording-widget-state", {
            state: "processing",
            level: 0,
            elapsedMs: 0,
            monochromatic: false,
          });
          element(stage, "pointer").classList.remove("pressed");
        },
      },
      {
        atMs: 1900,
        run: (stage) => {
          stage.setVisible("pill", false);
          element(stage, "pointer").style.opacity = "0";
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
