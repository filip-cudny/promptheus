import type { AppliedSkill, NodeUpdate, Source, ToolCall } from "$lib/types/ai";
import type { InvocationSource } from "$lib/types/history";

export interface ConversationNode {
  node_id: string;
  parent_id: string | null;
  role: "user" | "assistant";
  content: string;
  images: ConversationImage[];
  text_attachments: string[];
  timestamp: string;
  children: string[];
  updates: NodeUpdate[];
  prompt_tokens: number | null;
  completion_tokens: number | null;
  thinking: string | null;
  thinking_duration: number | null;
  query_duration: number | null;
  error: string | null;
  cancelled: boolean;
  tool_calls: ToolCall[];
  applied_skills: AppliedSkill[];
  sources: Source[];
}

export interface ConversationImage {
  data: string;
  media_type: string;
  number?: number;
}

export interface ConversationTree {
  nodes: Map<string, ConversationNode>;
  root_node_id: string | null;
  current_path: string[];
}

export interface MessagePair {
  user: ConversationNode;
  assistant: ConversationNode | null;
  message_number: number;
}

export interface TabState {
  tab_id: string;
  tab_name: string | null;
  invocation_source: InvocationSource;
  tree: ConversationTree;
  context_text: string;
  context_images: ConversationImage[];
  context_own: boolean;
  context_hint: boolean;
  context_bar_open: boolean;
  input_text: string;
  input_images: ConversationImage[];
  input_text_attachments: string[];
  is_executing: boolean;
  is_streaming: boolean;
  streamed_content: string;
  execution_id: string | null;
  history_entry_id: string | null;
  pristine_input: string | null;
  model_id: string | null;
  reasoning_effort: string | null;
  streamed_thinking: string;
  is_thinking: boolean;
  thinking_started_at: number | null;
  execution_started_at: number | null;
  active_tool_calls: ToolCall[];
  selected_tools: string[];
  abort_regenerate_node_id: string | null;
  shown: boolean;
}

export function hasUserContent(msg: {
  content: string;
  images: ConversationImage[];
  text_attachments: string[];
}): boolean {
  return (
    !!msg.content.trim() ||
    msg.images.length > 0 ||
    msg.text_attachments.length > 0
  );
}

export type ContentSegment =
  | { type: "text"; text: string }
  | { type: "tool_call"; tool_call_id: string };
