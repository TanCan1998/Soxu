const assert = require("node:assert/strict");
const fs = require("node:fs");

const args = process.argv.slice(2);
const session = "3344aabb-1234-4234-8234-3344aabbccdd";
const prompt = fs.readFileSync(0, "utf8");
const readOnlyArgs = [
  "--output-format", "stream-json", "--include-partial-messages", "--bare",
  "--safe-mode", "--approval-mode", "plan", "--max-tool-calls", "0",
  "--exclude-tools", "edit", "get_goal", "notebook_edit", "read_file", "run_shell_command", "update_goal",
];
assert.deepEqual(args.slice(0, readOnlyArgs.length), readOnlyArgs);
assert.deepEqual(
  args.slice(readOnlyArgs.length),
  ["fresh", "invalid"].includes(prompt) ? [] : ["--resume", session],
);

const sessionId = prompt === "invalid" ? "/untrusted/session.jsonl" : session;
const emit = (record) => process.stdout.write(`${JSON.stringify({ session_id: sessionId, ...record })}\n`);
emit({ type: "system", subtype: "init" });
emit({
  type: "stream_event",
  event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: `Qwen Code ${prompt}` } },
});
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else {
  const isError = prompt === "fail";
  emit({ type: "result", subtype: isError ? "error_during_execution" : "success", is_error: isError });
}
