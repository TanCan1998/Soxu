const assert = require("node:assert/strict");
const http = require("node:http");

const requests = [];
let release;
let stopDisconnected = false;
const server = http.createServer((request, response) => {
  if (request.url === "/release") {
    assert.ok(release);
    release();
    response.end("released");
    return;
  }
  if (request.url === "/state") {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify({ requests, stopDisconnected }));
    return;
  }
  let body = "";
  request.on("data", (chunk) => { body += chunk; });
  request.on("end", () => {
    assert.equal(request.url, "/v1/chat/completions");
    const payload = JSON.parse(body);
    assert.ok(!payload.tools || payload.tools.length === 0);
    const users = payload.messages.filter((message) => message.role === "user");
    const textOf = (message) => typeof message.content === "string" ? message.content
      : message.content.filter((part) => part.type === "text").map((part) => part.text).join("");
    const prompts = users.map(textOf);
    const prompt = prompts.at(-1);
    requests.push(prompts);
    assert.ok(["fresh", "hold", "correct", "queued", "stop"].includes(prompt));
    if (prompt !== "fresh") assert.ok(prompts.includes("fresh"));
    if (prompt === "correct") assert.ok(prompts.includes("hold"));
    if (prompt === "queued") assert.ok(prompts.includes("correct"));
    response.writeHead(200, { "content-type": "text/event-stream" });
    const emit = (delta, finish_reason = null) => response.write(`data: ${JSON.stringify({
      id: "chatcmpl-soxu", object: "chat.completion.chunk", created: 0, model: "soxu-pi",
      choices: [{ index: 0, delta, finish_reason }],
    })}\n\n`);
    emit({ role: "assistant", content: `SOXU_${prompt.toUpperCase()}` });
    const finish = () => { emit({}, "stop"); response.end("data: [DONE]\n\n"); };
    if (prompt === "hold") release = finish;
    else if (prompt === "stop") response.on("close", () => { stopDisconnected = true; });
    else finish();
  });
});
server.listen(0, "127.0.0.1", () => process.stdout.write(`${server.address().port}\n`));
