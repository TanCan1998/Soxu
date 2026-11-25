const http = require("node:http");
const fs = require("node:fs");
const record = (event) => fs.appendFileSync(process.env.SOXU_FIXTURE_REQUEST_LOG, `${event}\n`);

const server = http.createServer((request, response) => {
  let body = "";
  request.setEncoding("utf8");
  request.on("data", (chunk) => { body += chunk; });
  request.on("end", () => {
    if (request.method !== "POST" || !request.url.endsWith("/responses")) {
      record("other_route");
      response.writeHead(404).end();
      return;
    }

    record("responses_post");
    let input;
    try {
      input = JSON.parse(body).input;
      if (!Array.isArray(input)) throw new Error();
    } catch {
      record("invalid_body");
      response.writeHead(400).end();
      return;
    }
    const user = input.filter((item) => item.role === "user")
      .flatMap((item) => item.content.map((part) => part.text));
    const prompt = user.at(-1);
    const expected = process.env.SOXU_FIXTURE_PROMPT;
    let answer;
    if (prompt === expected) answer = "SOXU_NEW";
    if (prompt === `resume ${expected}` && user.includes(expected)) answer = "SOXU_RESUMED";
    if (prompt === `fork ${expected}` && user.includes(`resume ${expected}`)) answer = "SOXU_FORKED";
    if (!answer) {
      record("unexpected_prompt_or_history");
      response.writeHead(400, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: "unexpected test prompt or missing parent history" } }));
      return;
    }

    const id = `resp_${answer}`;
    const usage = {
      input_tokens: 10, output_tokens: 3, total_tokens: 13,
      input_tokens_details: { cached_tokens: 2 },
      output_tokens_details: { reasoning_tokens: 0 },
    };
    const events = [
      { type: "response.created", response: { id } },
      { type: "response.output_item.done", item: {
        type: "message", id: `msg_${answer}`, role: "assistant",
        content: [{ type: "output_text", text: answer }],
      } },
      { type: "response.completed", response: { id, usage } },
    ];
    response.writeHead(200, { "content-type": "text/event-stream" });
    for (const event of events) response.write(`data: ${JSON.stringify(event)}\n\n`);
    response.end();
    record(answer);
  });
});

server.listen(0, "127.0.0.1", () => process.stdout.write(`${server.address().port}\n`));
