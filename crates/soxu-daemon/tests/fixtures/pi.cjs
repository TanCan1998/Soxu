const assert = require("node:assert/strict");
const readline = require("node:readline");

const args = process.argv.slice(2);
const session = "eeddccbb-1234-4234-8234-eeddccbbaa99";
const emit = (record) => process.stdout.write(`${JSON.stringify(record)}\n`);
const readOnlyArgs = [
  "--mode", "rpc", "--no-tools", "--no-extensions", "--no-mcp",
  "--no-skills", "--no-prompt-templates", "--no-context-files",
  "--no-approve", "--offline",
];
assert.deepEqual(args.slice(0, readOnlyArgs.length), readOnlyArgs);

const input = readline.createInterface({ input: process.stdin });
input.on("line", (line) => {
  const command = JSON.parse(line);
  if (command.type === "get_state") {
    return;
  }
  if (command.type === "get_session_stats") {
    emit({ type: "response", id: command.id, success: true });
    return;
  }
  assert.equal(command.type, "prompt");
  const prompt = command.message;
  assert.deepEqual(
    args.slice(readOnlyArgs.length),
    ["fresh", "invalid"].includes(prompt) ? [] : ["--session", session],
  );
  emit({
    type: "response", id: "state", success: true,
    data: { sessionId: prompt === "invalid" ? "/untrusted/session.jsonl" : session },
  });
  emit({ type: "response", id: command.id, success: true });
  emit({ type: "agent_start" });
  emit({
    type: "message_update",
    assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: `Pi ${prompt}` },
  });
  if (prompt === "hold") {
    return;
  }
  emit({
    type: "message_end",
    message: { role: "assistant", stopReason: prompt === "fail" ? "error" : "stop" },
  });
  emit({ type: "agent_settled" });
});
