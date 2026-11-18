export interface WindowSpec {
  page: string;
  view: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

type FrameWindow = Window & { __screenshot?: { show?: () => Promise<void> } } & Record<string, unknown>;

export interface Stage {
  root: HTMLElement;
  frameWindow(name: string): FrameWindow;
  setVisible(name: string, visible: boolean): void;
  show(name: string): Promise<void>;
  press(name: string, key: string): void;
  sleep(ms: number): Promise<void>;
}

export interface Step {
  caption: string | null;
  shownAtMs: number;
  durationMs: number;
  actions: { atMs: number; run: (stage: Stage) => void | Promise<void> }[];
}

export interface Animation {
  backdrop: string;
  windows: Record<string, WindowSpec>;
  steps: Step[];
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

const animations = import.meta.glob<Animation>("./animations/*.ts");
const name = new URLSearchParams(location.search).get("animation") ?? "";
const load = animations[`./animations/${name}.ts`];
if (!load) throw new Error(`screenshot animation "${name}" has no file in animations/`);

const root = document.getElementById("stage") as HTMLElement;
const caption = document.getElementById("caption") as HTMLElement;
const frames = new Map<string, HTMLIFrameElement>();

const stage: Stage = {
  root,
  frameWindow: (windowName) => frames.get(windowName)!.contentWindow as FrameWindow,
  setVisible: (windowName, visible) => {
    frames.get(windowName)!.style.visibility = visible ? "visible" : "hidden";
  },
  show: async (windowName) => {
    await stage.frameWindow(windowName).__screenshot!.show!();
  },
  press: (windowName, key) => {
    const target = stage.frameWindow(windowName);
    for (const type of ["keydown", "keyup"]) {
      target.dispatchEvent(new (target as unknown as typeof globalThis).KeyboardEvent(type, { key, bubbles: true }));
    }
  },
  sleep,
};

function createFrame(windowName: string, spec: WindowSpec) {
  const frame = document.createElement("iframe");
  frame.src = `/${spec.page}?view=${spec.view}`;
  frame.style.cssText = `position:absolute;border:0;visibility:hidden;left:${spec.left}px;top:${spec.top}px;width:${spec.width}px;height:${spec.height}px`;
  frames.set(windowName, frame);
  root.append(frame);
  return new Promise<void>((resolve) => frame.addEventListener("load", () => resolve(), { once: true }));
}

async function prepare(animation: Animation) {
  root.insertAdjacentHTML("afterbegin", animation.backdrop);
  await Promise.all(Object.entries(animation.windows).map(([windowName, spec]) => createFrame(windowName, spec)));
  await document.fonts.ready;
  await sleep(500);
}

const animation = await load();
const ready = prepare(animation);

function setCaption(text: string | null) {
  caption.hidden = text === null;
  caption.textContent = text;
}

(window as unknown as { __animation: unknown }).__animation = {
  stepCount: animation.steps.length,
  ready,
  async beginStep(index: number) {
    const step = animation.steps[index];
    setCaption(step.caption);
    for (const action of step.actions) setTimeout(() => void action.run(stage), action.atMs);
    await sleep(step.shownAtMs);
  },
  async endStep(index: number) {
    const step = animation.steps[index];
    await sleep(step.durationMs - step.shownAtMs);
  },
};
