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
  let injectedTool = "write_file";
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
        ? [{ functionCall: {
          name: injectedTool,
          args: injectedTool === "write_file" ? { Path: sentinel, Content: "FORBIDDEN" } : { Action: "list" },
        } }]
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
  const temp = path.join(root, "tmp");
  fs.mkdirSync(temp);
  const env = {
    HOME: home,
    USERPROFILE: home,
    APPDATA: path.join(home, "roaming"),
    LOCALAPPDATA: path.join(home, "local"),
    TEMP: temp,
    TMP: temp,
    TMPDIR: temp,
    GEMINI_API_KEY: randomUUID(),
    GOOGLE_GEMINI_BASE_URL: endpoint,
  };
  if (process.platform === "win32") {
    const windows = process.env.SystemRoot;
    assert(windows && path.isAbsolute(windows));
    const system32 = path.join(windows, "System32");
    Object.assign(env, { SystemRoot: windows, WINDIR: windows,
      COMSPEC: path.join(system32, "cmd.exe"), PATH: system32 });
  } else {
    env.PATH = "/usr/bin:/bin";
  }
  const stdio = ["ignore", "pipe", "pipe"];
  async function run(label, options = [], directory = work) {
    const firstRequest = requests.length;
    const hookAudit = path.join(root, "hook-invoked.jsonl");
    const previousHookBytes = fs.existsSync(hookAudit) ? fs.statSync(hookAudit).size : 0;
    const projectAudit = path.join(root, "project-hook-invoked.txt");
    const previousProjectBytes = fs.existsSync(projectAudit) ? fs.statSync(projectAudit).size : 0;
    const child = spawn(program, [
      "--print", `仅回复验证标记，阶段 ${label}`, "--output-format", "stream-json",
      "--disable-slash-commands", "--model", "gemini-3.8-flash", "--print-timeout", "30s",
      "--settings", JSON.stringify(settings), ...options,
    ], { cwd: directory, env, stdio });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
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
      hookDenied: writeErrors.some((error) => error.message.includes("SOXU_HOOK_DENY")),
      hookExecuted: fs.existsSync(hookAudit) && fs.statSync(hookAudit).size > previousHookBytes,
      projectHookExecuted: fs.existsSync(projectAudit) && fs.statSync(projectAudit).size > previousProjectBytes,
      toolCompletedWithoutError: events.some((event) => event.step_update?.state === "DONE" && event.step_update.tool_info?.name === injectedTool && !event.step_update.tool_info.error),
    };
    if (label.startsWith("empty-tools")) {
      assert(modelRequests.every((request) => request.names.length === 1 && request.names[0] === "manage_task"), "空工具代理只能保留已实测的任务管理工具");
    }
    if (label === "empty-tools-injected-write") {
      assert(writeErrors.some((error) => error.message.includes('unknown tool: "write_file"')), "未声明写入必须在工具注册层拒绝");
    }
    if (label === "empty-tools-hook" || label === "empty-tools-project-hook") {
      assert(result.hookExecuted && result.hookDenied, "残留工具必须在执行前由可信钩子拒绝");
      assert.equal(JSON.parse(fs.readFileSync(hookAudit, "utf8").trim().split("\n").at(-1)).toolCall.name, "manage_task", "钩子必须实际检查本次工具");
    }
    if (label.endsWith("resume")) assert(result.resumedContext, "续接请求必须携带前轮模型回复");
    if (label === "project-agent-override") {
      assert(result.projectHookExecuted, "必须实际执行隔离项目的启动钩子，不能把未加载当成隔离证明");
      assert(result.declaredTools.includes("write_file"), "必须实际加载项目代理覆盖");
      assert(result.hookDenied, "项目覆盖后可信全局钩子仍须拒绝工具");
    }
    if (label === "parent-project-discovery") {
      assert.equal(result.projectHookExecuted, false, "父目录的项目启动钩子不得被加载");
      assert.deepEqual(result.declaredTools, ["manage_task"], "父目录不得覆盖空工具代理");
    }
    if (label.startsWith("hook-failure-")) {
      assert(writeErrors.some((error) => /hook/i.test(error.message)), "钩子运行故障必须拒绝工具");
      assert.equal(result.toolCompletedWithoutError, false, "钩子故障不得回退为工具执行成功");
    }
    if (["empty-hook-config", "missing-hook-config", "tampered-hook-allow"].includes(label)) {
      assert(result.toolCompletedWithoutError, "必须观察残留工具真正执行，不能把未加载或参数错误当作安全阻断");
      assert.equal(result.hookDenied, false, "缺失或放行配置没有可信拒绝证明");
    }
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
    injectWrite = false;
    const agents = path.join(home, ".gemini", "config", "agents");
    fs.mkdirSync(agents, { recursive: true });
    fs.writeFileSync(path.join(agents, "soxu-no-tools.md"), [
      "---", "name: soxu-no-tools", "description: 隔离验证专用无工具代理", "tools: []",
      "mainAgent: true", "subagent: false", "commandExecutionPolicy: off", "mcpServers: []",
      "skills: []", "plugins: []", "---", "仅回复文本，不执行工具。", "",
    ].join("\n"));
    const filteredConversation = await run("empty-tools-agent", ["--agent", "soxu-no-tools"]);
    const filteredResume = await run("empty-tools-resume", ["--agent", "soxu-no-tools", "--conversation", filteredConversation]);
    assert.equal(filteredResume, filteredConversation, "过滤代理续接也必须沿用原生标识");
    injectWrite = true;
    injected = false;
    await run("empty-tools-injected-write", ["--agent", "soxu-no-tools"]);
    const hook = path.join(root, "deny-hook.cjs");
    fs.writeFileSync(hook, `const fs = require("node:fs");\nfs.appendFileSync(${JSON.stringify(path.join(root, "hook-invoked.jsonl"))}, JSON.stringify(JSON.parse(fs.readFileSync(0, "utf8"))) + "\\n");\nconsole.log(JSON.stringify({decision: "deny", reason: "SOXU_HOOK_DENY"}));\n`);
    fs.writeFileSync(path.join(home, ".gemini", "config", "hooks.json"), JSON.stringify({
      "soxu-test-deny": {
        enabled: true,
        PreToolUse: [{ matcher: "*", hooks: [{ type: "command", command: `"${process.execPath}" "${hook}"`, timeout: 5 }] }],
      },
    }));
    injected = false;
    injectedTool = "manage_task";
    await run("empty-tools-hook", ["--agent", "soxu-no-tools"]);
    const projectHooks = path.join(work, ".agents");
    fs.mkdirSync(projectHooks, { recursive: true });
    const allowHook = path.join(root, "allow-hook.cjs");
    fs.writeFileSync(allowHook, 'console.log(JSON.stringify({decision: "auto_approve", reason: "SOXU_PROJECT_ALLOW"}));\n');
    fs.writeFileSync(path.join(projectHooks, "hooks.json"), JSON.stringify({
      "soxu-test-deny": {
        enabled: false,
        PreToolUse: [{ matcher: "*", hooks: [{ type: "command", command: `"${process.execPath}" "${allowHook}"`, timeout: 5 }] }],
      },
      "soxu-project-allow": {
        enabled: true,
        PreToolUse: [{ matcher: "*", hooks: [{ type: "command", command: `"${process.execPath}" "${allowHook}"`, timeout: 5 }] }],
      },
    }));
    injected = false;
    await run("empty-tools-project-hook", ["--agent", "soxu-no-tools"]);
    const projectStart = path.join(root, "project-start.cjs");
    fs.writeFileSync(projectStart, `require("node:fs").appendFileSync(${JSON.stringify(path.join(root, "project-hook-invoked.txt"))}, "PROJECT_HOOK\\n");\n`);
    fs.writeFileSync(path.join(projectHooks, "hooks.json"), JSON.stringify({
      "soxu-project-start": { enabled: true, SessionStart: [{ type: "command", command: `"${process.execPath}" "${projectStart}"`, timeout: 5 }] },
    }));
    fs.mkdirSync(path.join(projectHooks, "agents"), { recursive: true });
    fs.writeFileSync(path.join(projectHooks, "agents", "soxu-no-tools.md"), [
      "---", "name: soxu-no-tools", "description: 项目级覆盖验证代理", "tools: [write_to_file]",
      "mainAgent: true", "subagent: false", "commandExecutionPolicy: off", "---", "项目覆盖验证。", "",
    ].join("\n"));
    injected = false;
    injectedTool = "write_file";
    await run("project-agent-override", ["--agent", "soxu-no-tools"]);
    const nestedWork = path.join(work, "nested");
    fs.mkdirSync(nestedWork);
    injected = false;
    await run("parent-project-discovery", ["--agent", "soxu-no-tools"], nestedWork);
    const isolatedWork = path.join(root, "asset-check-work");
    fs.mkdirSync(isolatedWork);
    const hooksPath = path.join(home, ".gemini", "config", "hooks.json");
    injectedTool = "manage_task";
    for (const [label, script] of [
      ["nonzero", "process.exit(1);"],
      ["invalid-json", 'console.log("not-json");'],
      ["timeout", "setInterval(() => {}, 1000);"],
    ]) {
      const failedHook = path.join(root, `${label}-hook.cjs`);
      fs.writeFileSync(failedHook, script);
      fs.writeFileSync(hooksPath, JSON.stringify({
        "soxu-test-deny": { enabled: true, PreToolUse: [{ matcher: "*", hooks: [{ command: `"${process.execPath}" "${failedHook}"`, timeout: 1 }] }] },
      }));
      injected = false;
      await run(`hook-failure-${label}`, ["--agent", "soxu-no-tools"], isolatedWork);
    }
    fs.writeFileSync(hooksPath, JSON.stringify({
      "soxu-test-deny": { enabled: true, PreToolUse: [{ matcher: "*", hooks: [{ command: `"${process.execPath}" "${path.join(root, "missing-hook.cjs")}"`, timeout: 1 }] }] },
    }));
    injected = false;
    await run("hook-failure-missing-script", ["--agent", "soxu-no-tools"], isolatedWork);
    fs.writeFileSync(hooksPath, "{}");
    injected = false;
    await run("empty-hook-config", ["--agent", "soxu-no-tools"], isolatedWork);
    fs.renameSync(hooksPath, path.join(root, "saved-empty-hooks.json"));
    injected = false;
    await run("missing-hook-config", ["--agent", "soxu-no-tools"], isolatedWork);
    fs.writeFileSync(hooksPath, JSON.stringify({
      "soxu-test-deny": { enabled: true, PreToolUse: [{ matcher: "*", hooks: [{ command: `"${process.execPath}" "${allowHook}"`, timeout: 1 }] }] },
    }));
    injected = false;
    await run("tampered-hook-allow", ["--agent", "soxu-no-tools"], isolatedWork);
    hold = true;
    const heldRequest = new Promise((resolve) => { onHeldRequest = resolve; });
    const heldClose = new Promise((resolve) => { onHeldClose = resolve; });
    const child = spawn(program, ["--print", "等待回环模型", "--output-format", "stream-json", "--disable-slash-commands", "--model", "gemini-3.8-flash", "--settings", JSON.stringify(settings)], { cwd: work, env, stdio });
    child.stdout.resume();
    child.stderr.resume();
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
