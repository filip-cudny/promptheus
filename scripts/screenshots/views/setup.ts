import type { Fixture } from "../mocks";
import { appSettings, fixture as settingsFixture } from "./settings";

export const label = "settings-dialog";

const defaultBindings = (mod: string) => ({
  [`${mod}+f1`]: "open_context_menu",
  [`${mod}+f2`]: "execute_active_prompt",
  [`${mod}+f3`]: "set_context_value",
  "shift+f3": "append_context_value",
  "shift+f4": "clear_context",
  "shift+f1": "speech_to_text_toggle",
});

export const fixture: Fixture = {
  ...settingsFixture,
  get_settings: {
    ...appSettings,
    keymaps: [
      { context: "os == macos", bindings: defaultBindings("cmd") },
      { context: "os == linux", bindings: defaultBindings("ctrl") },
    ],
  },
  get_setup_status: { active: true, step: 4 },
  get_env_vars: appSettings.env_vars.map(({ name, description, secret }) => ({
    name,
    description,
    secret,
    set: true,
    used_in: [],
  })),
  get_settings_sync_status: { state: "off", message: null, repo_path: null, branch: null, last_sync: null },
};

async function until(condition: () => boolean, what: string) {
  for (let waited = 0; !condition(); waited += 50) {
    if (waited > 5000) throw new Error(`setup: ${what} within 5 s`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

export async function show() {
  const { getSettingsNavigation } = await import("$lib/stores/settingsNavigation.svelte");
  const navigation = getSettingsNavigation();
  await until(() => navigation.highlightedSection !== null, "the banner did not open its section");
  await until(() => navigation.highlightedSection === null, "the section highlight did not fade");
}
