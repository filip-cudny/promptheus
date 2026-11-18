import type { Fixture } from "../mocks";
import { fixture as menuFixture } from "./context-menu";

export { label, show } from "./context-menu";

export const fixture: Fixture = {
  ...menuFixture,
  hide_context_menu_panel: null,
  execute_skill: null,
};
