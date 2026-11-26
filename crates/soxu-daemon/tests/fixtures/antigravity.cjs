const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const session = "aabbccdd-1234-4234-8234-aabbccddeeff";
const args = process.argv.slice(2);
const prompt = args.find((arg) => arg.startsWith("--print=")).slice("--print=".length);
const managed = path.join(process.env.HOME, ".gemini", "config");
assert(fs.readFileSync(path.join(managed, "agents", "soxu-no-tools.md"), "utf8").includes("tools: []"));
assert(fs.readFileSync(path.join(managed, "hooks.json"), "utf8").includes("PreToolUse"));
if (args.includes("--conversation")) assert(args.includes(session));
console.log(JSON.stringify({ event: "step_update", step_update: {
  conversation_id: session, step_type: "agent_response", text_delta: prompt,
} }));
if (prompt === "hold") {
  setInterval(() => {}, 1000);
} else {
  console.log(JSON.stringify({ event: "result", result: { conversation_id: session, status: "SUCCESS", response: prompt } }));
}
