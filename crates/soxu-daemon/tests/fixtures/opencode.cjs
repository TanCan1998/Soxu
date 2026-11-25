const assert = require("node:assert/strict");
const fs = require("node:fs");

const session = "ses_dedf8357effemZrzPmvFfyljxn";
const prompt = fs.readFileSync(0, "utf8");
const fresh = ["fresh", "invalid"].includes(prompt);
const args = ["run", "--format", "json"];
assert.deepEqual(process.argv.slice(2), fresh ? args : [...args, "--session", session]);

const sessionID = prompt === "invalid" ? "ses_../outside" : session;
const emit = (type, part) => process.stdout.write(`${JSON.stringify({ type, sessionID, part })}\n`);
emit("step_start", { type: "step-start" });
emit("text", { type: "text", text: `OpenCode ${prompt}` });
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else if (prompt === "fail") {
  emit("error");
} else {
  emit("step_finish", { type: "step-finish", reason: "stop" });
}
