import { execFile, spawn } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";

const MAX_FPS = 15;
const WEBP_QUALITY = 75;
const hostPollLimit = 300;

export function ffmpegPath() {
  return process.env.FFMPEG || "ffmpeg";
}

export async function checkFfmpeg() {
  const ffmpeg = ffmpegPath();
  let encoders;
  try {
    ({ stdout: encoders } = await promisify(execFile)(ffmpeg, ["-hide_banner", "-encoders"]));
  } catch (e) {
    throw new Error(`ffmpeg is not runnable at ${ffmpeg} (set FFMPEG to its path): ${e.message.split("\n")[0]}`);
  }
  if (!encoders.includes("libwebp_anim")) throw new Error(`ffmpeg at ${ffmpeg} lacks the libwebp_anim encoder`);
}

function runFfmpeg(args) {
  const ffmpeg = ffmpegPath();
  return new Promise((resolve, reject) => {
    const child = spawn(ffmpeg, ["-hide_banner", "-loglevel", "error", "-y", ...args], { stdio: ["ignore", "ignore", "pipe"] });
    let stderr = "";
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", (e) => reject(new Error(`ffmpeg at ${ffmpeg} failed to start: ${e.message}`)));
    child.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`ffmpeg at ${ffmpeg} exited ${code}: ${stderr.trim()}`))));
  });
}

export function describeWebp(buffer) {
  const stats = { frames: 0, width: 0, durationMs: 0, loops: null };
  for (let at = 12; at + 8 <= buffer.length; ) {
    const type = buffer.toString("ascii", at, at + 4);
    const size = buffer.readUInt32LE(at + 4);
    const body = at + 8;
    if (type === "VP8X") stats.width = buffer.readUIntLE(body + 4, 3) + 1;
    if (type === "ANIM") stats.loops = buffer.readUInt16LE(body + 4);
    if (type === "ANMF") {
      stats.frames++;
      stats.durationMs += buffer.readUIntLE(body + 12, 3);
    }
    at = body + size + (size % 2);
  }
  return stats;
}

function concatList(frames, endedAt) {
  const lines = ["ffconcat version 1.0"];
  frames.forEach((frame, i) => {
    const until = i + 1 < frames.length ? frames[i + 1].startedAt : endedAt;
    lines.push(`file '${frame.file}'`, `duration ${((until - frame.startedAt) / 1000).toFixed(3)}`);
  });
  lines.push(`file '${frames.at(-1).file}'`);
  return lines.join("\n");
}

export async function renderAnimation({ cdp, sessionId, url, name, spec, stepsDir, loadPage, evaluate }) {
  const framesDir = mkdtempSync(join(tmpdir(), "screenshots-frames-"));
  try {
    await cdp.send(
      "Emulation.setDeviceMetricsOverride",
      { width: spec.width, height: spec.height, deviceScaleFactor: 2, mobile: false },
      sessionId,
    );
    await loadPage(cdp, sessionId, url);
    await evaluate(
      cdp,
      sessionId,
      `new Promise((resolve, reject) => {
        let polls = 0;
        const timer = setInterval(() => {
          if (window.__animation) { clearInterval(timer); resolve(window.__animation.ready); }
          else if (++polls > ${hostPollLimit}) { clearInterval(timer); reject(new Error("host page did not start the animation")); }
        }, 50);
      })`,
    );
    const stepCount = await evaluate(cdp, sessionId, "window.__animation.stepCount");
    if (stepsDir) mkdirSync(stepsDir, { recursive: true });

    const frames = [];
    let markedStep = null;
    let capturing = true;
    const captureLoop = (async () => {
      while (capturing) {
        const startedAt = performance.now();
        const step = markedStep;
        markedStep = null;
        const { data } = await cdp.send("Page.captureScreenshot", { format: "png" }, sessionId);
        const file = join(framesDir, `frame-${String(frames.length).padStart(4, "0")}.png`);
        writeFileSync(file, Buffer.from(data, "base64"));
        frames.push({ file, startedAt });
        if (step !== null && stepsDir) copyFileSync(file, join(stepsDir, `${name}-${step}.png`));
        await delay(Math.max(0, 1000 / MAX_FPS - (performance.now() - startedAt)));
      }
    })();

    try {
      for (let index = 0; index < stepCount; index++) {
        await evaluate(cdp, sessionId, `window.__animation.beginStep(${index})`);
        markedStep = index + 1;
        await evaluate(cdp, sessionId, `window.__animation.endStep(${index})`);
      }
    } finally {
      capturing = false;
      await captureLoop;
    }
    const endedAt = performance.now();

    const listFile = join(framesDir, "frames.txt");
    writeFileSync(listFile, concatList(frames, endedAt));
    const file = join(framesDir, `${name}.webp`);
    await runFfmpeg([
      "-f", "concat", "-safe", "0", "-i", listFile,
      "-fps_mode", "vfr", "-c:v", "libwebp_anim", "-q:v", String(WEBP_QUALITY), "-loop", "0",
      file,
    ]);
    return readFileSync(file);
  } finally {
    rmSync(framesDir, { recursive: true, force: true });
  }
}
