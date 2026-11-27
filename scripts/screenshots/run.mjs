import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { createServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { parseCatalog } from "./catalog.mjs";
import { checkFfmpeg, describeWebp, renderAnimation } from "./animate.mjs";

const root = join(import.meta.dirname, "..", "..");
const outDir = join(root, "docs", "screenshots");
const mocksPath = "/scripts/screenshots/mocks.ts";
const catalogModule = "virtual:screenshots/catalog";
const builtinEnvKeys = ["BASE_URL", "DEV", "MODE", "PROD", "SSR"];
const settleQuietMs = 500;
const settleTimeoutMs = 15000;

const VIEWS = {
  settings: { page: "settings-dialog.html", width: 1340, height: 640 },
  "context-menu": { page: "context-menu.html", width: 320, height: 416, clip: ".menu-shell" },
  conversation: { page: "conversation-dialog.html", width: 700, height: 800 },
  palette: { page: "palette.html", width: 520, height: 400, clip: ".palette-modal" },
  history: { page: "history-dialog.html", width: 600, height: 500, type: "search" },
  setup: { page: "settings-dialog.html", width: 960, height: 680 },
  "recording-pill": {
    page: "recording-widget.html",
    width: 264,
    height: 72,
    backdrop: "rgb(128, 128, 128)",
    margin: 12,
  },
};

const ANIMATIONS = {
  "quick-action": { width: 800, height: 540 },
  dictation: { width: 800, height: 560 },
};

function chromePath() {
  if (process.env.CHROME) return process.env.CHROME;
  if (process.platform === "darwin") return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  return "google-chrome";
}

function selectedTargets() {
  const args = process.argv.slice(2);
  let stepsDir;
  const stepsAt = args.indexOf("--steps");
  if (stepsAt !== -1) {
    const [, value] = args.splice(stepsAt, 2);
    if (!value || value.startsWith("--")) throw new Error("--steps needs a directory");
    stepsDir = resolve(value);
  }
  const unknown = args.filter((name) => !(name in VIEWS) && !(name in ANIMATIONS));
  if (unknown.length) {
    throw new Error(
      `unknown name ${unknown.join(", ")}; views: ${Object.keys(VIEWS).join(", ")}; animations: ${Object.keys(ANIMATIONS).join(", ")}`,
    );
  }
  const names = args.length ? args : [...Object.keys(VIEWS), ...Object.keys(ANIMATIONS)];
  const animations = names.filter((name) => name in ANIMATIONS);
  if (stepsDir && !animations.length) throw new Error("--steps needs an animation name");
  return { views: names.filter((name) => name in VIEWS), animations, stepsDir };
}

function mocksPlugin(catalog) {
  return {
    name: "screenshot-mocks",
    resolveId: (id) => (id === catalogModule ? `\0${catalogModule}` : undefined),
    load: (id) => (id === `\0${catalogModule}` ? `export default ${JSON.stringify(catalog)};` : undefined),
    transformIndexHtml: (html) => ({
      html: html.replace(
        /<script type="module" src="\/src\/windows\//,
        `<script type="module" src="${mocksPath}"></script>\n    $&`,
      ),
      tags: [{ tag: "link", attrs: { rel: "icon", href: "data:," }, injectTo: "head" }],
    }),
  };
}

async function startServer(catalog, envDir) {
  for (const key of Object.keys(process.env)) if (key.startsWith("VITE_")) delete process.env[key];
  const server = await createServer({
    configFile: false,
    root,
    envDir,
    cacheDir: join(root, "node_modules", ".vite-screenshots"),
    plugins: [svelte(), mocksPlugin(catalog)],
    resolve: { alias: { $lib: join(root, "src", "lib") } },
    optimizeDeps: { entries: ["**/*.html", "scripts/screenshots/mocks.ts"] },
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await server.listen();
  return server;
}

async function readDevToolsUrl(profile, chrome) {
  const portFile = join(profile, "DevToolsActivePort");
  for (let waited = 0; waited < 20000; waited += 100) {
    if (chrome.exitCode !== null) throw new Error(`Chrome exited with ${chrome.exitCode}: ${chrome.stderrTail}`);
    if (existsSync(portFile)) {
      const [port, path] = readFileSync(portFile, "utf8").split("\n");
      if (port && path) return `ws://127.0.0.1:${port}${path}`;
    }
    await delay(100);
  }
  throw new Error(`Chrome wrote no ${portFile} within 20 s: ${chrome.stderrTail}`);
}

function launchChrome(profile) {
  const chrome = spawn(
    chromePath(),
    [
      "--headless=new",
      `--user-data-dir=${profile}`,
      "--remote-debugging-port=0",
      "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
      "--disable-background-networking",
      "--disable-component-update",
      "--no-first-run",
      "--no-default-browser-check",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  chrome.stderrTail = "";
  chrome.stderr.on("data", (chunk) => (chrome.stderrTail = (chrome.stderrTail + chunk).slice(-2000)));
  chrome.exited = new Promise((resolve) => chrome.on("exit", resolve));
  chrome.on("error", (e) => (chrome.stderrTail += String(e)));
  return chrome;
}

async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", () => reject(new Error(`cannot connect to ${url}`)), { once: true });
  });
  let nextId = 1;
  const pending = new Map();
  const listeners = new Map();
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.id !== undefined) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(`${message.error.message} (${message.error.code})`));
      else resolve(message.result);
    } else {
      listeners.get(`${message.sessionId}:${message.method}`)?.(message.params);
    }
  });
  return {
    send(method, params = {}, sessionId) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params, sessionId }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    on(sessionId, method, listener) {
      listeners.set(`${sessionId}:${method}`, listener);
    },
    close: () => socket.close(),
  };
}

function isAllowedUrl(raw) {
  const url = new URL(raw);
  if (url.protocol === "data:" || url.protocol === "blob:") return true;
  return (url.protocol === "http:" || url.protocol === "ws:") && url.hostname === "127.0.0.1";
}

function describeConsoleCall({ args }) {
  return args.map((arg) => String(arg.value ?? arg.description ?? arg.type).split("\n")[0]).join(" ");
}

function watchFailures(cdp, sessionId) {
  const failures = [];
  const checkUrl = ({ url, request }) => {
    const target = url ?? request.url;
    if (!isAllowedUrl(target)) failures.push(`request outside 127.0.0.1: ${target}`);
  };
  cdp.on(sessionId, "Runtime.exceptionThrown", ({ exceptionDetails }) =>
    failures.push(`page exception: ${(exceptionDetails.exception?.description ?? exceptionDetails.text).split("\n")[0]}`),
  );
  cdp.on(sessionId, "Runtime.consoleAPICalled", (call) => {
    if (call.type === "error") failures.push(`console error: ${describeConsoleCall(call)}`);
  });
  cdp.on(sessionId, "Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") failures.push(`log error: ${entry.text} ${entry.url ?? ""}`.trim());
  });
  cdp.on(sessionId, "Network.requestWillBeSent", checkUrl);
  cdp.on(sessionId, "Network.webSocketCreated", checkUrl);
  return failures;
}

async function evaluate(cdp, sessionId, expression) {
  const { result, exceptionDetails } = await cdp.send(
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    sessionId,
  );
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
}

const settleExpression = `new Promise((resolve) => {
  let timer;
  const done = () => { observer.disconnect(); clearTimeout(limit); resolve(); };
  const observer = new MutationObserver(() => { clearTimeout(timer); timer = setTimeout(done, ${settleQuietMs}); });
  const limit = setTimeout(done, ${settleTimeoutMs});
  observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  document.fonts.ready.then(() => { timer = setTimeout(done, ${settleQuietMs}); });
})`;

function clipExpression(selector) {
  return `(() => {
    const r = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect();
    return r && { x: r.x, y: r.y, width: r.width, height: r.height, scale: 1 };
  })()`;
}

function backdropExpression(view, margin) {
  const body = { boxSizing: "content-box", width: `${view.width}px`, height: `${view.height}px`, padding: `${margin}px` };
  return `document.documentElement.style.background = ${JSON.stringify(view.backdrop)};
    Object.assign(document.body.style, ${JSON.stringify(body)});`;
}

async function loadPage(cdp, sessionId, url) {
  const loaded = new Promise((resolve) => cdp.on(sessionId, "Page.loadEventFired", resolve));
  const { errorText } = await cdp.send("Page.navigate", { url }, sessionId);
  if (errorText) throw new Error(`navigation to ${url} failed: ${errorText}`);
  await loaded;
}

async function captureView(cdp, sessionId, url, name) {
  const view = VIEWS[name];
  const margin = view.margin ?? 0;
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: view.width + 2 * margin, height: view.height + 2 * margin, deviceScaleFactor: 2, mobile: false },
    sessionId,
  );
  if (view.clip) {
    await cdp.send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } }, sessionId);
  }
  await loadPage(cdp, sessionId, url);
  if (view.backdrop) await evaluate(cdp, sessionId, backdropExpression(view, margin));
  await evaluate(cdp, sessionId, "window.__screenshot.show?.()");
  if (view.type) await cdp.send("Input.insertText", { text: view.type }, sessionId);
  await evaluate(cdp, sessionId, settleExpression);
  const envKeys = await evaluate(cdp, sessionId, "window.__screenshot.envKeys");
  console.log(`${name}: import.meta.env keys ${envKeys.join(", ")}`);
  const extraKeys = envKeys.filter((key) => !builtinEnvKeys.includes(key));
  if (extraKeys.length) throw new Error(`import.meta.env holds ${extraKeys.join(", ")}`);
  const clip = view.clip ? await evaluate(cdp, sessionId, clipExpression(view.clip)) : undefined;
  if (view.clip && !clip) throw new Error(`no element matches ${view.clip}`);
  const { data } = await cdp.send("Page.captureScreenshot", { format: "png", clip }, sessionId);
  return data;
}

async function withTarget(cdp, name, capture) {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  try {
    const failures = watchFailures(cdp, sessionId);
    let result;
    try {
      for (const domain of ["Page", "Runtime", "Log", "Network"]) await cdp.send(`${domain}.enable`, {}, sessionId);
      result = await capture(sessionId);
    } catch (e) {
      failures.push(e.message.split("\n")[0]);
    }
    if (failures.length) throw new Error(`${name}: ${failures.join("; ")}`);
    return result;
  } finally {
    await cdp.send("Target.closeTarget", { targetId });
  }
}

async function renderView(cdp, baseUrl, name) {
  const data = await withTarget(cdp, name, (sessionId) =>
    captureView(cdp, sessionId, `${baseUrl}/${VIEWS[name].page}?view=${name}`, name),
  );
  const file = join(outDir, `${name}.png`);
  writeFileSync(file, Buffer.from(data, "base64"));
  console.log(`${name}: wrote ${file}`);
}

async function renderAnimationView(cdp, baseUrl, name, stepsDir) {
  const webp = await withTarget(cdp, name, (sessionId) =>
    renderAnimation({
      cdp,
      sessionId,
      url: `${baseUrl}/scripts/screenshots/host.html?animation=${name}`,
      name,
      spec: ANIMATIONS[name],
      stepsDir,
      loadPage,
      evaluate,
    }),
  );
  const file = join(outDir, `${name}.webp`);
  writeFileSync(file, webp);
  const { frames, width, durationMs, loops } = describeWebp(webp);
  console.log(`${name}: wrote ${file}`);
  console.log(`${name}: ${frames} frames, ${width} px wide, ${(durationMs / 1000).toFixed(2)} s, loop count ${loops}`);
}

async function main() {
  const { views, animations, stepsDir } = selectedTargets();
  if (animations.length) await checkFfmpeg();
  const catalog = parseCatalog(readFileSync(join(root, "src-tauri", "src", "services", "config", "catalog.rs"), "utf8"));
  const envDir = mkdtempSync(join(tmpdir(), "screenshots-env-"));
  const profile = mkdtempSync(join(tmpdir(), "screenshots-chrome-"));
  let server;
  let chrome;
  let cdp;
  const cleanup = async () => {
    cdp?.close();
    if (chrome && chrome.exitCode === null) {
      chrome.kill();
      await chrome.exited;
    }
    await server?.close();
    rmSync(profile, { recursive: true, force: true });
    rmSync(envDir, { recursive: true, force: true });
  };
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => cleanup().finally(() => process.exit(1)));
  }
  try {
    server = await startServer(catalog, envDir);
    const { port } = server.httpServer.address();
    chrome = launchChrome(profile);
    cdp = await connect(await readDevToolsUrl(profile, chrome));
    mkdirSync(outDir, { recursive: true });
    const baseUrl = `http://127.0.0.1:${port}`;
    for (const name of views) await renderView(cdp, baseUrl, name);
    for (const name of animations) await renderAnimationView(cdp, baseUrl, name, stepsDir);
  } finally {
    await cleanup();
  }
}

main().catch((e) => {
  console.error(`screenshots failed: ${e.message}`);
  process.exitCode = 1;
});
