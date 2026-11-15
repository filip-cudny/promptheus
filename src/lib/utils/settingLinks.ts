import type { CatalogSection } from "../types/settingsCatalog";

const SECTION_PREFIX = "section:";
const SECTION_IDS = [
  "general",
  "notifications",
  "history",
  "sync",
  "environment",
  "models",
  "prompt_base",
  "skills",
  "mcp_servers",
  "chat",
  "context_menu",
  "speech",
  "shortcuts",
] as const satisfies readonly CatalogSection[];

const SETTING_SCHEME = "setting:";
const SETTING_LINK = /\[[^\]]*\]\((setting:[^)\s]+)\)/g;

export function parseSettingLink(href: string): string | null {
  if (!href.startsWith(SETTING_SCHEME)) return null;
  const key = decodeURIComponent(href.slice(SETTING_SCHEME.length));
  return key || null;
}

export function showSettingLinks(markdown: string): string {
  return markdown.replace(SETTING_LINK, "[show]($1)");
}

export function isSectionKey(key: string): boolean {
  return key.startsWith(SECTION_PREFIX);
}

export function parseSectionLink(key: string): CatalogSection | null {
  if (!isSectionKey(key)) return null;
  const id = key.slice(SECTION_PREFIX.length);
  return SECTION_IDS.find((section) => section === id) ?? null;
}
