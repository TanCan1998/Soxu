const assert = require("node:assert/strict");
const fs = require("node:fs");

assert.ok(process.argv.includes('sandbox_mode="read-only"'));
const prompt = fs.readFileSync(0, "utf8");
fs.appendFileSync(process.env.SOXU_TEST_LAUNCH_LOG, `${prompt}\n`);
const emit = (record) => process.stdout.write(`${JSON.stringify(record)}\n`);
emit({ type: "thread.started", thread_id: "aabbccdd-1234-4234-8234-aabbccddeeff" });
emit({ type: "item.completed", item: { type: "agent_message", text: prompt } });
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else {
  setTimeout(() => emit({ type: "turn.completed", usage: {} }), prompt === "delayed" ? 1500 : 0);
}
