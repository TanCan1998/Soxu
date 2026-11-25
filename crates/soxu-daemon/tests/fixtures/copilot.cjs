const assert = require("node:assert/strict");
const fs = require("node:fs");

const args = process.argv.slice(2);
const session = "aabbccdd-1234-4234-8234-aabbccddeeff";
const prompt = fs.readFileSync(0, "utf8");
const fresh = ["fresh", "invalid"].includes(prompt);
const readOnlyArgs = [
  "--output-format", "json", "--no-ask-user", "--disable-builtin-mcps",
  "--no-auto-update", "--no-custom-instructions", "--disable-client-telemetry",
  "--available-tools=soxu_no_native_tools", "--stream", "on",
];
assert.deepEqual(args, fresh ? readOnlyArgs : [...readOnlyArgs, `--resume=${session}`]);

const sessionId = prompt === "invalid" ? "/untrusted/session.jsonl" : session;
const emit = (type, data) => process.stdout.write(`${JSON.stringify({ type, data })}\n`);
emit(fresh ? "session.start" : "session.resume", fresh ? { sessionId } : {});
emit("assistant.turn_start", { turnId: "0" });
emit("assistant.message_delta", { messageId: "message-0", deltaContent: `Copilot ${prompt}` });
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else {
  emit("assistant.turn_end", { turnId: "0" });
  emit("assistant.idle", {});
  const exitCode = prompt === "fail" ? 7 : 0;
  process.stdout.write(`${JSON.stringify({ type: "result", sessionId, exitCode })}\n`);
}
