const assert = require("node:assert/strict");
const fs = require("node:fs");
const readline = require("node:readline");

const args = process.argv.slice(2);
const kind = process.env.SOXU_FIXTURE_KIND;
const mode = process.env.SOXU_FIXTURE_MODE;
const prompt = process.env.SOXU_FIXTURE_PROMPT;
const session = "aabbccdd-1234-4234-8234-aabbccddeeff";
const emit = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);
assert.equal(fs.realpathSync(process.cwd()), fs.realpathSync(process.env.SOXU_FIXTURE_CWD));

function codexText(text) {
  emit({ type: "item.completed", item: { type: "agent_message", text } });
}

if (kind === "pi") {
  const reader = readline.createInterface({ input: process.stdin });
  reader.on("line", (line) => {
    const request = JSON.parse(line);
    const { id, type } = request;
    if (type === "get_state") {
      emit({ id, type: "response", command: type, success: true, data: { sessionId: session, model: { contextWindow: 4096 } } });
    } else if (type === "get_session_stats") {
      emit({ id, type: "response", command: type, success: true, data: { contextUsage: { tokens: 123, contextWindow: 4096 } } });
    } else if (type === "prompt") {
      assert.equal(request.message, prompt);
      assert.deepEqual(args.slice(-2), ["--fork", require("node:path").join(process.cwd(), "prior session with spaces & metadata.jsonl")]);
      emit({ id, type: "response", command: type, success: true, data: { disposition: "started" } });
      emit({ type: "agent_start" });
    } else {
      assert.equal(type, { 1: "follow_up", 2: "steer", 3: "abort" }[id]);
      if (type !== "abort") assert.equal(request.message, prompt);
      if (type === "abort") {
        emit({ type: "message_end", message: { role: "assistant", stopReason: "aborted" } });
        emit({ type: "agent_settled" });
      }
      emit({ id, type: "response", command: type, success: true });
    }
  });
} else {
  if (["codex", "claude", "qwen", "copilot", "opencode"].includes(kind)) {
    assert.equal(fs.readFileSync(0, "utf8"), prompt);
  } else {
    assert.equal(args.at(-1), `--print=${prompt}`);
  }
  switch (kind) {
    case "codex":
      emit({ type: "thread.started", thread_id: session });
      emit({ type: "turn.started" });
      if (mode === "hold") {
        setInterval(() => {}, 1000);
        break;
      }
      if (mode === "fail") {
        process.stderr.write("private diagnostics\n");
        emit({ type: "turn.failed", error: { message: "private diagnostics" } });
        process.exitCode = 7;
        break;
      }
      if (mode === "truncated") {
        process.stdout.write('{"type":');
        break;
      }
      if (mode === "overflow") process.stdout.write(`${"x".repeat(4 * 1024 * 1024 + 1)}\n`);
      process.stdout.write("private diagnostic outside protocol\n");
      if (process.env.SOXU_FIXTURE_SESSION) {
        assert.deepEqual(args.slice(-2), [session, "-"]);
        assert(args.includes(process.env.SOXU_FIXTURE_SESSION));
      }
      codexText(prompt);
      emit({ type: "turn.completed", usage: { input_tokens: 10, cached_input_tokens: 2, output_tokens: 3 } });
      break;
    case "claude":
    case "qwen":
      emit({ type: "system", subtype: kind === "claude" ? "init" : "session_start", session_id: session });
      emit({ type: "stream_event", event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: prompt } } });
      emit({ type: "assistant", message: { content: [{ type: "text", text: prompt }] } });
      emit({ type: "result", subtype: "success" });
      break;
    case "agy":
      emit({ event: "step_update", step_update: { conversation_id: session, step_type: "agent_response", text_delta: prompt } });
      emit({ event: "result", result: { conversation_id: session, status: "SUCCESS", response: prompt, usage: { total_tokens: 13 } } });
      break;
    case "copilot":
      emit({ type: "session.start", data: { sessionId: session } });
      emit({ type: "assistant.message_delta", data: { messageId: "message-0", deltaContent: prompt } });
      emit({ type: "assistant.message", data: { messageId: "message-0", content: prompt } });
      emit({ type: "assistant.turn_end", data: { turnId: "0" } });
      emit({ type: "session.shutdown", data: {} });
      break;
    case "opencode":
      assert.deepEqual(args, ["run", "--format", "json"]);
      emit({ type: "step_start", sessionID: session, part: { type: "step-start" } });
      emit({ type: "text", sessionID: session, part: { type: "text", text: prompt } });
      emit({ type: "step_finish", sessionID: session, part: { type: "step-finish", reason: "stop", tokens: { input: 10, output: 3 } } });
      break;
  }
}
