import { info } from "@tauri-apps/plugin-log";

type Switches = { noScroll: boolean; delayResize: boolean };
type Kind = "expand" | "collapse";
type MarkData = Record<string, number | string | boolean | null>;
type Entry = { t: number; name: string } & MarkData;
type Frame = {
  t: number;
  innerHeight: number;
  bodyScrollTop: number;
  docScrollTop: number;
  menuTop: number | null;
};
type Session = {
  kind: Kind;
  entries: Entry[];
  frames: Frame[];
  startedAt: number;
  onScroll: (e: Event) => void;
  frameRequest: number;
  flushTimer: ReturnType<typeof setTimeout>;
};

declare global {
  interface Window {
    __ctxMenuDiag?: Partial<Switches>;
  }
}

const SAMPLE_MS = 600;
const FLUSH_MS = 700;

const switches: Switches = {
  noScroll: window.__ctxMenuDiag?.noScroll === true,
  delayResize: window.__ctxMenuDiag?.delayResize === true,
};

if (switches.noScroll) {
  const style = document.createElement("style");
  style.textContent = "html, body { overflow: clip; }";
  document.head.appendChild(style);
}

let session: Session | null = null;

function now() {
  return performance.timeOrigin + performance.now();
}

function describeTarget(target: EventTarget | null) {
  if (target instanceof Element) {
    return { target: target.tagName, targetClass: target.getAttribute("class") ?? "", scrollTop: target.scrollTop };
  }
  return { target: "document", targetClass: "", scrollTop: document.scrollingElement?.scrollTop ?? 0 };
}

function sampleFrame(s: Session) {
  s.frames.push({
    t: now(),
    innerHeight: window.innerHeight,
    bodyScrollTop: document.body.scrollTop,
    docScrollTop: document.documentElement.scrollTop,
    menuTop: document.querySelector(".menu-shell")?.getBoundingClientRect().top ?? null,
  });
  if (now() - s.startedAt < SAMPLE_MS) {
    s.frameRequest = requestAnimationFrame(() => sampleFrame(s));
  }
}

function flush() {
  const s = session;
  if (!s) return;
  session = null;
  document.removeEventListener("scroll", s.onScroll, { capture: true });
  cancelAnimationFrame(s.frameRequest);
  clearTimeout(s.flushTimer);
  void info(
    `[ctx-menu-diag] ${JSON.stringify({ kind: s.kind, switches, entries: s.entries, frames: s.frames })}`,
  );
}

export function ctxMenuDiagSwitches(): Switches {
  return { ...switches };
}

export function ctxMenuDiagBegin(kind: Kind) {
  flush();
  const startedAt = now();
  const s: Session = {
    kind,
    entries: [{ t: startedAt, name: "click", kind }],
    frames: [],
    startedAt,
    onScroll: (e) => s.entries.push({ t: now(), name: "scroll", ...describeTarget(e.target) }),
    frameRequest: 0,
    flushTimer: setTimeout(flush, FLUSH_MS),
  };
  session = s;
  document.addEventListener("scroll", s.onScroll, { capture: true });
  s.frameRequest = requestAnimationFrame(() => sampleFrame(s));
}

export function ctxMenuDiagMark(name: string, data: MarkData = {}) {
  session?.entries.push({ t: now(), name, ...data });
}
