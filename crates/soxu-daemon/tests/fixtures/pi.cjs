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
let currentPrompt;
const settle = () => {
  emit({ type: "message_end", message: { role: "assistant", stopReason: "stop" } });
  emit({ type: "agent_settled" });
};
input.on("line", (line) => {
  const command = JSON.parse(line);
  if (command.type === "get_state") {
    return;
  }
  if (command.type === "get_session_stats") {
    if (currentPrompt === "settled-hold") return;
    emit({ type: "response", id: command.id, success: true });
    return;
  }
  if (command.type === "steer") {
    assert.equal(currentPrompt, "hold");
    if (command.message === "no-ack") return;
    if (command.message === "exit-no-ack") process.exit(0);
    if (command.message === "late-ack") {
      settle();
      setTimeout(() => emit({ type: "response", id: command.id, success: true }), 100);
      return;
    }
    const accepted = command.message !== "reject";
    emit({ type: "response", id: command.id, success: accepted });
    if (accepted) {
      emit({ type: "message_update", assistantMessageEvent: {
        type: "text_delta", contentIndex: 0, delta: ` Pi steer ${command.message}`,
      } });
    }
    if (command.message === "finish") settle();
    return;
  }
  assert.equal(command.type, "prompt");
  const prompt = command.message;
  currentPrompt = prompt;
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
  if (prompt === "settled-hold") {
    settle();
    return;
  }
  emit({
    type: "message_end",
    message: { role: "assistant", stopReason: prompt === "fail" ? "error" : "stop" },
  });
  emit({ type: "agent_settled" });
});
