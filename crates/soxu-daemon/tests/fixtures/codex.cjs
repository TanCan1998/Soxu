const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const args = process.argv.slice(2);
const session = "aabbccdd-1234-4234-8234-aabbccddeeff";
const prompt = fs.readFileSync(0, "utf8");
const emit = (record) => process.stdout.write(`${JSON.stringify(record)}\n`);

assert.deepEqual(args.slice(0, 5), [
  "--ask-for-approval", "never", "-c", 'sandbox_mode="read-only"', "exec",
]);
if (prompt === "hold" || prompt === "directory") {
  assert.deepEqual(args.slice(-3), ["--sandbox", "read-only", "-"]);
} else {
  assert.equal(args[5], "resume");
  assert.deepEqual(args.slice(-2), [session, "-"]);
}
if (prompt === "directory") {
  assert.equal(path.basename(process.cwd()), "project-fixture");
} else {
  assert.match(path.basename(process.cwd()), /^qc_[0-9a-f]{32}$/);
  assert.equal(path.basename(path.dirname(process.cwd())), "quick-chats");
}

emit({ type: "thread.started", thread_id: session });
emit({ type: "turn.started" });
if (prompt === "fail") {
  process.stderr.write("private diagnostic\n");
  emit({ type: "turn.failed", error: { message: "private diagnostic" } });
  process.exitCode = 7;
} else {
  emit({ type: "item.completed", item: { type: "agent_message", text: `回复 ${prompt}` } });
  if (prompt === "hold") {
    setInterval(() => {}, 1000);
  } else {
    emit({ type: "turn.completed", usage: { input_tokens: 10, output_tokens: 3 } });
  }
}
