export type CatalogSection =
  | "general"
  | "notifications"
  | "history"
  | "sync"
  | "environment"
  | "models"
  | "prompt_base"
  | "skills"
  | "mcp_servers"
  | "chat"
  | "context_menu"
  | "speech"
  | "shortcuts";

export type CatalogKind =
  | { type: "bool" }
  | { type: "integer"; min: number | null; max: number | null }
  | { type: "text"; max_len: number | null }
  | { type: "choice"; values: string[] }
  | { type: "list" }
  | { type: "map" }
  | { type: "model"; model_type: "text" | "stt" }
  | { type: "shortcut" }
  | { type: "composite" };

export interface CatalogEntry {
  key: string;
  section: CatalogSection;
  label: string;
  description: string;
  kind: CatalogKind;
  applies: "live" | "after_restart";
}
