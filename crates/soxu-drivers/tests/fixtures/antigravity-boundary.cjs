const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { randomUUID } = require("node:crypto");

async function main() {
  const source = process.env.SOXU_TEST_AGY;
  assert(source && path.isAbsolute(source), "SOXU_TEST_AGY 必须是可信程序的绝对路径");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "soxu-agy-boundary-"));
  const home = path.join(root, "home");
  const work = path.join(root, "work");
  fs.mkdirSync(home);
  fs.mkdirSync(work);
  const program = path.join(root, process.platform === "win32" ? "antigravity-cli.exe" : "agy");
  fs.copyFileSync(source, program);
  if (process.platform !== "win32") fs.chmodSync(program, 0o700);
  const sentinel = path.join(work, "forbidden.txt");
  const requests = [];
  let injectWrite = false;
  let injected = false;
  let hold = false;
  let onHeldRequest;
  let onHeldClose;
  const server = http.createServer((request, response) => {
    let input = "";
    request.on("data", (chunk) => { input += chunk; });
    request.on("end", () => {
      const body = JSON.parse(input);
      const declarations = (body.tools || []).flatMap((tool) => tool.functionDeclarations || tool.function_declarations || []);
      requests.push({ names: declarations.map((tool) => tool.name), contents: body.contents });
      if (hold) {
        response.on("close", () => onHeldClose?.());
        onHeldRequest?.();
        return;
      }
      const parts = injectWrite && !injected
        ? [{ functionCall: { name: "write_file", args: { Path: sentinel, Content: "FORBIDDEN" } } }]
        : [{ text: "SOXU_AGY_OK" }];
      injected ||= injectWrite;
      const payload = {
        candidates: [{ content: { role: "model", parts }, finishReason: "STOP", index: 0 }],
        usageMetadata: { promptTokenCount: 12, candidatesTokenCount: 4, totalTokenCount: 16 },
        modelVersion: "gemini-3.8-flash",
        responseId: "soxu-fixture",
      };
      response.writeHead(200, { "Content-Type": "text/event-stream" });
      response.end(`data: ${JSON.stringify(payload)}\n\n`);
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const endpoint = `http://127.0.0.1:${server.address().port}`;
  const settings = {
    modelProvider: "gemini",
    toolPermission: "strict",
    allowNonWorkspaceAccess: false,
    permissions: { deny: ["read_file(*)", "write_file(*)", "command(*)", "read_url(*)", "execute_url(*)", "mcp(*)"] },
  };
  const env = {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    APPDATA: path.join(home, "roaming"),
    LOCALAPPDATA: path.join(home, "local"),
    GEMINI_API_KEY: randomUUID(),
    GOOGLE_GEMINI_BASE_URL: endpoint,
  };
  delete env.GOOGLE_API_KEY;
  delete env.GOOGLE_APPLICATION_CREDENTIALS;
  delete env.GOOGLE_CLOUD_PROJECT;
  async function run(label, options = []) {
    const firstRequest = requests.length;
    const child = spawn(program, [
      "--print", `仅回复验证标记，阶段 ${label}`, "--output-format", "stream-json",
      "--disable-slash-commands", "--model", "gemini-3.8-flash", "--print-timeout", "30s",
      "--settings", JSON.stringify(settings), ...options,
    ], { cwd: work, env });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.stdin.end();
    const deadline = setTimeout(() => child.kill(), 45000);
    const exitCode = await new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("exit", resolve);
    });
    clearTimeout(deadline);
    const events = stdout.split("\n").flatMap((line) => {
      try { return [JSON.parse(line)]; } catch { return []; }
    });
    const modelRequests = requests.slice(firstRequest);
    assert(modelRequests.length > 0, `必须真正调用回环模型，退出码=${exitCode}`);
    assert.equal(fs.existsSync(sentinel), false, "全拒绝配置下不得写入目标文件");
    const final = events.find((event) => event.event === "result")?.result;
    const writeErrors = events.filter((event) => event.step_update?.tool_info?.error)
      .map((event) => event.step_update.tool_info.error)
      .map((error) => ({ ...error, message: error.message.replaceAll(root.replaceAll("\\", "/"), "<fixture>").replaceAll(root, "<fixture>") }));
    assert.equal(exitCode, 0, "回合必须正常退出");
    assert.equal(final?.status, "SUCCESS", "必须收到成功终态");
    if (label === "injected-write") {
      assert(writeErrors.some((error) => /Matches user-configured deny rule/.test(error.message)), "必须由权限拒绝，而不是参数无效导致未执行");
    }
    const result = {
      label,
      exitCode,
      declaredTools: modelRequests[0].names,
      requestCount: modelRequests.length,
      result: final.status,
      conversationId: final.conversation_id,
      resumedContext: modelRequests.some((request) => (request.contents || []).some((content) => content.role === "model" && JSON.stringify(content.parts).includes("SOXU_AGY_OK"))),
      writeErrors,
      permissionNotice: /denied|permission|blocked/i.test(stderr),
      sentinelCreated: false,
      zeroToolBoundary: modelRequests.every((request) => request.names.length === 0),
    };
    if (label === "resume") assert(result.resumedContext, "续接请求必须携带前轮模型回复");
    fs.writeFileSync(path.join(root, `${label}.json`), JSON.stringify(result, null, 2));
    console.log(JSON.stringify(result));
    return final.conversation_id;
  }
  try {
    console.log(JSON.stringify({ evidenceDirectory: root }));
    const conversation = await run("strict-deny");
    assert(conversation, "必须取得原生会话标识");
    const resumed = await run("resume", ["--conversation", conversation]);
    assert.equal(resumed, conversation, "续接必须沿用同一原生会话");
    injectWrite = true;
    await run("injected-write");
    hold = true;
    const heldRequest = new Promise((resolve) => { onHeldRequest = resolve; });
    const heldClose = new Promise((resolve) => { onHeldClose = resolve; });
    const child = spawn(program, ["--print", "等待回环模型", "--output-format", "stream-json", "--disable-slash-commands", "--model", "gemini-3.8-flash", "--settings", JSON.stringify(settings)], { cwd: work, env });
    child.stdout.resume();
    child.stderr.resume();
    child.stdin.end();
    const exited = new Promise((resolve) => child.on("exit", resolve));
    const deadline = setTimeout(() => child.kill(), 30000);
    try {
      await Promise.race([heldRequest, exited.then(() => { throw new Error("模型请求前进程已退出"); })]);
      child.kill();
      await exited;
      await Promise.race([heldClose, new Promise((_, reject) => setTimeout(() => reject(new Error("取消后模型连接未断开")), 5000))]);
      console.log(JSON.stringify({ label: "cancel", modelConnectionClosed: true }));
    } finally {
      clearTimeout(deadline);
      child.kill();
    }
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
