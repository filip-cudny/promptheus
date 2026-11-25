import type { Source, StreamEvent, ToolCall } from "$lib/types/ai";

export interface AgentReply {
  text: string;
  toolCalls: ToolCall[];
  thinkingParts: string[];
  thinking: string | null;
  isThinking: boolean;
  iterationText: string;
  secretViews: string[];
  sources: Source[];
}

export function emptyReply(): AgentReply {
  return { text: "", toolCalls: [], thinkingParts: [], thinking: null, isThinking: false, iterationText: "", secretViews: [], sources: [] };
}

export function applyStreamEvent(reply: AgentReply, event: StreamEvent) {
  switch (event.event) {
    case "chunk":
      reply.text = event.data.accumulated;
      if (event.data.accumulated_thinking != null) reply.thinking = event.data.accumulated_thinking;
      reply.isThinking = event.data.accumulated_thinking != null && event.data.accumulated === reply.iterationText;
      break;
    case "tool_call_start":
      if (reply.thinking != null) {
        reply.thinkingParts.push(reply.thinking);
        reply.thinking = null;
      }
      reply.text += `{{tool_call:${event.data.tool_call.tool_call_id}}}`;
      reply.iterationText = reply.text;
      reply.isThinking = false;
      reply.toolCalls.push({ ...event.data.tool_call });
      break;
    case "tool_call_done": {
      const call = reply.toolCalls.find((c) => c.tool_call_id === event.data.tool_call_id);
      if (call) {
        call.result = event.data.result;
        call.error = event.data.error;
        call.status = event.data.error ? "failed" : "completed";
        call.completed_at = new Date().toISOString();
      }
      break;
    }
    case "done":
      reply.text = event.data.full_text;
      if (event.data.full_thinking != null) reply.thinking = event.data.full_thinking;
      reply.sources = event.data.sources ?? [];
      reply.isThinking = false;
      break;
  }
}

export function thinkingText(reply: AgentReply): string {
  const parts = reply.thinking != null ? [...reply.thinkingParts, reply.thinking] : reply.thinkingParts;
  return parts.join("\n\n");
}

const FINISHED_STATUSES = ["completed", "failed", "cancelled"];

export function showsProcessing(reply: AgentReply, running: boolean): boolean {
  if (!running || reply.isThinking) return false;
  const waitingForContent =
    !reply.text &&
    !thinkingText(reply) &&
    !reply.toolCalls.some((call) => call.status === "in_progress" || call.status === "pending");
  const processingToolResults =
    reply.toolCalls.length > 0 && reply.toolCalls.every((call) => FINISHED_STATUSES.includes(call.status));
  return waitingForContent || processingToolResults;
}
