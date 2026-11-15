import type { ChangeLogEntry, UndoConflict } from "$lib/services/changeLog";
import type { EffectTarget } from "$lib/services/settingsAgent";
import type { CatalogEntry } from "../types/settingsCatalog";
import type { ModelConfig } from "../types";
import { catalogPath, listItemOf, type ListItemRef } from "./settingsSearch.ts";

export function targetLabel(target: EffectTarget): string {
  return target.type === "secret" ? target.name : target.path;
}

export function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "(none)";
  return typeof value === "string" ? value : JSON.stringify(value);
}

export function formatEntryTime(createdAt: string): string {
  return new Date(createdAt).toLocaleString();
}

const LIST_NOUNS: Record<ListItemRef["list"], string> = {
  models: "models",
  skills: "skills",
  mcp_servers: "MCP servers",
};

const MODEL_ID_PREFIX_LENGTH = 8;

function itemName(item: ListItemRef, models: ModelConfig[]): string {
  if (item.list !== "models") return item.id;
  const model = models.find((m) => m.id === item.id);
  return model ? model.display_name || model.model : item.id.slice(0, MODEL_ID_PREFIX_LENGTH);
}

function findEntry(catalog: CatalogEntry[], path: string): CatalogEntry | undefined {
  const key = catalogPath(path);
  return catalog.find((e) => e.key === key);
}

export function effectPathLabel(target: EffectTarget, catalog: CatalogEntry[], models: ModelConfig[]): string {
  if (target.type !== "setting") return targetLabel(target);
  const entry = findEntry(catalog, target.path);
  if (!entry) return targetLabel(target);
  const item = listItemOf(target.path);
  return item ? `${itemName(item, models)}: ${entry.label}` : entry.label;
}

interface SettingGroup {
  path: string;
  label: string;
  items: Map<string, ListItemRef>;
}

function settingGroupTitle(
  group: SettingGroup,
  catalog: CatalogEntry[],
  models: ModelConfig[],
): string {
  const entry = findEntry(catalog, group.path);
  if (!entry) return group.label;
  const [first] = group.items.values();
  if (!first) return entry.label;
  if (group.items.size === 1) return `${itemName(first, models)}: ${entry.label}`;
  return `${entry.label}: ${group.items.size} ${LIST_NOUNS[first.list]}`;
}

function cardTitleParts(entry: ChangeLogEntry, catalog: CatalogEntry[], models: ModelConfig[]): string[] {
  const parts: (string | SettingGroup)[] = [];
  const groups = new Map<string, SettingGroup>();
  for (const op of entry.operations) {
    const paths = op.op === "set_setting" ? op.effects.flatMap((e) => (e.target.type === "setting" ? [e.target.path] : [])) : [];
    if (paths.length === 0) {
      parts.push(op.label);
      continue;
    }
    for (const path of paths) {
      const key = catalogPath(path);
      let group = groups.get(key);
      if (!group) {
        group = { path, label: op.label, items: new Map() };
        groups.set(key, group);
        parts.push(group);
      }
      const item = listItemOf(path);
      if (item) group.items.set(`${item.list}:${item.id}`, item);
    }
  }
  return parts.map((p) => (typeof p === "string" ? p : settingGroupTitle(p, catalog, models)));
}

export function entryTitle(
  entry: ChangeLogEntry,
  entries: ChangeLogEntry[],
  catalog: CatalogEntry[],
  models: ModelConfig[],
  formatTime: (createdAt: string) => string = formatEntryTime,
): string {
  if (entry.kind === "secret") return `Secret ${entry.secret?.name ?? ""}`.trim();
  if (entry.kind === "undo") {
    const undone = entries.find((e) => e.id === entry.undoes);
    return undone ? `Undo of ${formatTime(undone.created_at)}` : "Undo of an earlier entry";
  }
  return cardTitleParts(entry, catalog, models).join(", ") || "Change";
}

export function conflictLines(conflicts: UndoConflict[]): string[] {
  return conflicts.map((c) =>
    c.target.type === "secret"
      ? `${targetLabel(c.target)}: changed since this entry`
      : `${targetLabel(c.target)}: current ${formatValue(c.current)}, expected ${formatValue(c.expected_after)}`,
  );
}
