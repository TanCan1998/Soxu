const assert = require("node:assert/strict");
const fs = require("node:fs");

const args = process.argv.slice(2);
const session = "7788eeff-1234-4234-8234-7788eeff9900";
const prompt = fs.readFileSync(0, "utf8");
const readOnlyArgs = [
  "--print", "--verbose", "--output-format", "stream-json", "--include-partial-messages",
  "--tools", "", "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}',
  "--safe-mode", "--restricted", "--disable-slash-commands",
];
assert.deepEqual(args.slice(0, readOnlyArgs.length), readOnlyArgs);
assert.deepEqual(
  args.slice(readOnlyArgs.length),
  ["fresh", "invalid"].includes(prompt) ? [] : ["--resume", session],
);

const sessionId = prompt === "invalid" ? "/untrusted/session.jsonl" : session;
const emit = (record) => process.stdout.write(`${JSON.stringify({ session_id: sessionId, ...record })}\n`);
emit({ type: "system", subtype: "init", tools: [], mcp_servers: [] });
emit({
  type: "stream_event",
  event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: `Claude Code ${prompt}` } },
});
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else {
  const isError = prompt === "fail";
  emit({ type: "result", subtype: isError ? "error_during_execution" : "success", is_error: isError });
}
