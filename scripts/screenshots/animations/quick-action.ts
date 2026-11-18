import type { Animation, Step } from "../host";

const original = "Wir treffen uns morgen um neun am Bahnhof.";
const translation = "We are meeting tomorrow at nine at the station.";
const modelName = "GPT-6";

export const backdrop = `
<style>
  .desktop { position: absolute; inset: 0; background: linear-gradient(135deg, #3d4b63, #232a38); }
  .editor { position: absolute; left: 40px; top: 20px; width: 720px; height: 440px; background: #fbfaf7; border-radius: 10px; box-shadow: 0 18px 50px rgba(0, 0, 0, 0.45); overflow: hidden; color: #2b2b2b; }
  .editor-bar { height: 34px; background: #e6e3dc; display: flex; align-items: center; justify-content: center; font-size: 13px; color: #6b6760; }
  .editor-text { padding: 26px 34px; font-size: 19px; line-height: 1.7; }
  .editor-text p { margin: 0 0 14px; }
  .editor-text mark { background: #b7d4ff; color: inherit; border-radius: 2px; }
</style>
<div class="desktop"></div>
<div class="editor">
  <div class="editor-bar">weekend-trip.txt</div>
  <div class="editor-text">
    <p>Notes for the weekend trip.</p>
    <p><mark id="selection">${original}</mark></p>
    <p>Pack the tent, two sleeping bags and the small stove.</p>
    <p>Call Anna about the car once the plan is confirmed.</p>
  </div>
</div>`;

export const windows: Animation["windows"] = {
  menu: { page: "context-menu.html", view: "quick-action-menu", left: 456, top: 124, width: 320, height: 416 },
  toast: { page: "notification.html", view: "notification", left: 460, top: 340, width: 340, height: 140 },
};

export const steps: Step[] = [
  { caption: "Ctrl+F1", shownAtMs: 400, durationMs: 1500, actions: [] },
  {
    caption: null,
    shownAtMs: 900,
    durationMs: 1700,
    actions: [
      {
        atMs: 200,
        run: async (stage) => {
          await stage.show("menu");
          await stage.sleep(150);
          stage.setVisible("menu", true);
        },
      },
    ],
  },
  {
    caption: "2",
    shownAtMs: 400,
    durationMs: 1000,
    actions: [
      {
        atMs: 0,
        // Simplification: the app clears the selected row in closeMenu() in the same tick as the digit press, so no frame can show row 2 highlighted as the digit's result; the arrow presses highlight it beforehand.
        // Limit: the highlight appears before the digit press, not from it. Stops being correct when the menu keeps the row highlighted while it closes; then drop the arrow presses.
        run: (stage) => {
          stage.press("menu", "ArrowDown");
          stage.press("menu", "ArrowDown");
        },
      },
      {
        atMs: 800,
        run: (stage) => {
          stage.press("menu", "2");
          stage.setVisible("menu", false);
        },
      },
    ],
  },
  { caption: null, shownAtMs: 300, durationMs: 1500, actions: [] },
  {
    caption: null,
    shownAtMs: 700,
    durationMs: 1900,
    actions: [
      {
        atMs: 0,
        run: async (stage) => {
          const toast = stage.frameWindow("toast");
          toast.pendingNotifications = [
            {
              id: "quick-action-done",
              level: "success",
              title: "Translate to English",
              message: `${modelName} · 1.4s · copied to clipboard`,
              monochromatic: false,
              action: null,
            },
          ];
          stage.setVisible("toast", true);
          await (toast.drainPending as () => Promise<void>)();
        },
      },
    ],
  },
  {
    caption: "Ctrl+V",
    shownAtMs: 600,
    durationMs: 2600,
    actions: [
      {
        atMs: 300,
        run: (stage) => stage.root.querySelector("#selection")!.replaceWith(translation),
      },
    ],
  },
];
