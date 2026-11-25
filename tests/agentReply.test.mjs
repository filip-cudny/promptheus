import test from "node:test";
import assert from "node:assert";
import { applyStreamEvent, emptyReply, showsProcessing, thinkingText } from "../src/lib/utils/agentReply.ts";

function chunk(accumulated, accumulatedThinking) {
  return {
    event: "chunk",
    data: { delta: "", accumulated, thinking_delta: null, accumulated_thinking: accumulatedThinking },
  };
}

function toolCall(id) {
  return {
    tool_call_id: id,
    tool_name: "t",
    tool_display_name: "t",
    tool_type: "custom",
    arguments: {},
    result: null,
    error: null,
    status: "in_progress",
    started_at: null,
    completed_at: null,
  };
}

test("thinking is active until text arrives and stays in the thinking text", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("", "A"));
  assert.strictEqual(thinkingText(reply), "A");
  assert.strictEqual(reply.isThinking, true);
  applyStreamEvent(reply, chunk("x", "A"));
  assert.strictEqual(reply.isThinking, false);
  assert.strictEqual(thinkingText(reply), "A");
});

test("a tool call closes the iteration and the next thinking is appended", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("", "A"));
  applyStreamEvent(reply, chunk("x", "A"));
  applyStreamEvent(reply, { event: "tool_call_start", data: { tool_call: toolCall("c1") } });
  applyStreamEvent(reply, chunk("x{{tool_call:c1}}", "B"));
  assert.strictEqual(thinkingText(reply), "A\n\nB");
  assert.strictEqual(reply.isThinking, true);
  assert.strictEqual(reply.toolCalls.length, 1);
});

test("done does not duplicate the last iteration's thinking", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("", "A"));
  applyStreamEvent(reply, chunk("x", "A"));
  applyStreamEvent(reply, { event: "tool_call_start", data: { tool_call: toolCall("c1") } });
  applyStreamEvent(reply, chunk("x{{tool_call:c1}}", "B"));
  applyStreamEvent(reply, {
    event: "done",
    data: { full_text: "x{{tool_call:c1}}y", full_thinking: "B", prompt_tokens: null, completion_tokens: null },
  });
  assert.strictEqual(thinkingText(reply), "A\n\nB");
  assert.strictEqual(reply.isThinking, false);
  assert.strictEqual(reply.text, "x{{tool_call:c1}}y");
});

test("tool_call_done with an error marks the call failed", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, { event: "tool_call_start", data: { tool_call: toolCall("c1") } });
  applyStreamEvent(reply, { event: "tool_call_done", data: { tool_call_id: "c1", result: null, error: "boom" } });
  assert.strictEqual(reply.toolCalls[0].status, "failed");
  assert.strictEqual(reply.toolCalls[0].error, "boom");
});

function start(reply, id) {
  applyStreamEvent(reply, { event: "tool_call_start", data: { tool_call: toolCall(id) } });
}

test("tool_call_start appends the call's marker to the text", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("x", null));
  start(reply, "c1");
  assert.strictEqual(reply.text, "x{{tool_call:c1}}");
});

test("a thinking-only chunk after tool_call_start keeps the reply thinking", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("x", "A"));
  start(reply, "c1");
  applyStreamEvent(reply, chunk("x{{tool_call:c1}}", "B"));
  assert.strictEqual(reply.isThinking, true);
});

test("done with the same marker leaves exactly one marker in the text", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("x", "A"));
  start(reply, "c1");
  applyStreamEvent(reply, {
    event: "done",
    data: { full_text: "x{{tool_call:c1}}y", full_thinking: "B", prompt_tokens: null, completion_tokens: null },
  });
  assert.strictEqual(reply.text.split("{{tool_call:c1}}").length - 1, 1);
});

test("showsProcessing is true for an empty reply while running", () => {
  assert.strictEqual(showsProcessing(emptyReply(), true), true);
});

test("showsProcessing is false while a call is in progress", () => {
  const reply = emptyReply();
  start(reply, "c1");
  assert.strictEqual(showsProcessing(reply, true), false);
});

test("showsProcessing is true when all calls are done", () => {
  const reply = emptyReply();
  start(reply, "c1");
  applyStreamEvent(reply, { event: "tool_call_done", data: { tool_call_id: "c1", result: "ok", error: null } });
  assert.strictEqual(showsProcessing(reply, true), true);
});

test("showsProcessing is false while thinking", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, chunk("", "A"));
  assert.strictEqual(showsProcessing(reply, true), false);
});

test("showsProcessing is false when not running", () => {
  assert.strictEqual(showsProcessing(emptyReply(), false), false);
});

test("emptyReply has no sources", () => {
  assert.deepStrictEqual(emptyReply().sources, []);
});

test("done leaves its sources on the reply in order", () => {
  const reply = emptyReply();
  const sources = [
    { url: "https://a.example", title: "A" },
    { url: "https://b.example", title: "B" },
  ];
  applyStreamEvent(reply, {
    event: "done",
    data: { full_text: "x", full_thinking: null, prompt_tokens: null, completion_tokens: null, sources },
  });
  assert.deepStrictEqual(reply.sources, sources);
});

test("done without the sources key leaves no sources", () => {
  const reply = emptyReply();
  applyStreamEvent(reply, {
    event: "done",
    data: { full_text: "x", full_thinking: null, prompt_tokens: null, completion_tokens: null },
  });
  assert.deepStrictEqual(reply.sources, []);
});
