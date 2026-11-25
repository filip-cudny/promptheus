import type { ReasoningLevel } from "$lib/constants/models";
import type { ConversationImage } from "$lib/types/conversation";

export type ReasoningMode =
  | { kind: "unsupported" }
  | { kind: "effort"; allowed: ReasoningLevel[] }
  | { kind: "budget_tokens"; min: number; max: number }
  | { kind: "toggle" };

export interface ModelCapabilities {
  reasoning: ReasoningMode;
}

export type ToolCallStatus = "pending" | "in_progress" | "completed" | "failed" | "cancelled";

export type ToolCallType = "web_search" | "code_execution" | "file_read" | "file_write" | "api_call" | "custom";

export interface ToolCall {
  tool_call_id: string;
  tool_name: string;
  tool_display_name: string;
  tool_type: ToolCallType;
  arguments: Record<string, unknown>;
  result: string | null;
  error: string | null;
  status: ToolCallStatus;
  started_at: string | null;
  completed_at: string | null;
}

export interface Source {
  url: string;
  title: string;
}

export type NodeUpdate =
  | { type: "environment"; value: string }
  | { type: "context"; content: string; reason: string; image_refs: string[] };

export type StreamEvent =
  | { event: "chunk"; data: { delta: string; accumulated: string; thinking_delta: string | null; accumulated_thinking: string | null } }
  | { event: "done"; data: { full_text: string; full_thinking: string | null; prompt_tokens: number | null; completion_tokens: number | null; sources: Source[] } }
  | { event: "error"; data: { message: string } }
  | { event: "node_updates"; data: { node_id: string; updates: NodeUpdate[] } }
  | { event: "tool_call_start"; data: { tool_call: ToolCall } }
  | { event: "tool_call_done"; data: { tool_call_id: string; result: string | null; error: string | null } };

export interface AppliedSkill {
  name: string;
  skill_version_id: number;
  input: string;
}

export interface ConversationNodeForExecution {
  node_id: string;
  role: string;
  content: string;
  images: ConversationImage[];
  text_attachments: string[];
  updates: NodeUpdate[];
  applied_skills: AppliedSkill[];
}
