/**
 * 13 组视口矩阵「真实浏览器渲染」断言（Issue #123 验收）。
 *
 * 前置：`pnpm --dir apps/mobile-web run build`（用已构建 dist）。
 * 运行：`node apps/mobile-web/scripts/viewport-matrix.mjs`
 *
 * 实现方式（零新增依赖）：
 * 1. 用 `vite preview` 伺服 dist；
 * 2. 用系统 Edge headless + DevTools Protocol（HTTP `/json/list` 发现目标，
 *    WebSocket 帧用 Node 内置能力手写最小客户端）逐视口设置真实
 *    `Emulation.setDeviceMetricsOverride`；
 * 3. 每视口读取 `document.documentElement.scrollWidth/clientWidth`，
 *    断言无水平溢出；并校验引导页主按钮实际 bounds 位于视口内。
 *
 * 证据范围（如实区分，不夸大）：
 * - 无 daemon 时应用渲染「登录前 pairing 引导页」，本脚本覆盖该页面的全矩阵；
 * - 已认证 shell（双屏/Sink 侧栏/抽屉分支）需真实 daemon 配对后再验，
 *   不在本脚本范围内；
 * - 若 Edge / CDP 不可用，脚本以退出码 2 明确降级并打印限制，不冒充通过。
 */

import { spawn } from "node:child_process";
import { accessSync, mkdtempSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { get as httpGet, request as httpRequest } from "node:http";

const EDGE_CANDIDATES = [
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "/usr/bin/microsoft-edge",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];

const MATRIX = [
  ["iPad 1/3 分屏", 320, 1180, "compact"],
  ["小折叠外屏", 344, 882, "compact"],
  ["iPhone SE 小屏", 375, 667, "compact"],
  ["主流手机竖屏", 393, 852, "compact"],
  ["大折叠展开", 412, 882, "compact"],
  ["手机横屏", 852, 393, "regular"],
  ["折叠屏半折桌面态", 840, 480, "regular"],
  ["折叠屏内屏展开", 840, 900, "regular"],
  ["iPad 竖屏", 820, 1180, "regular"],
  ["iPad 13 竖屏", 1024, 1366, "wide"],
  ["iPad 13 横屏", 1180, 820, "wide"],
  ["Android 自由分屏代表点", 500, 900, "compact"],
  ["200% 字号缩放最小视口", 320, 1180, "compact"],
];

const PREVIEW_PORT = 4175;
const CDP_PORT = 9231;
// vite preview 在某些平台只绑定 IPv6 回环，探测时优先 localhost 回退 127.0.0.1
const PROBE_URLS = [`http://localhost:${PREVIEW_PORT}/`, `http://127.0.0.1:${PREVIEW_PORT}/`];
const APP_URL = `http://localhost:${PREVIEW_PORT}/`;

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function httpJson(url, timeoutMs = 3000) {
  return new Promise((res, rej) => {
    const req = httpGet(url, { timeout: timeoutMs, family: 0 }, (resp) => {
      let body = "";
      resp.on("data", (c) => (body += c));
      resp.on("end", () => {
        try {
          res(JSON.parse(body));
        } catch {
          // 非 JSON（如 index.html）也视为服务已就绪
          res({ __raw: body.slice(0, 40) });
        }
      });
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", rej);
  });
}

/** 最小 WebSocket 客户端（客户端帧必须 MASK；CDP 只发文本帧）。 */
class MinimalWs {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.waiters = [];
    socket.on("data", (chunk) => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      this.drain();
    });
  }

  drain() {
    while (this.waiters.length > 0) {
      const frame = this.readFrame();
      if (!frame) return;
      this.waiters.shift()(frame);
    }
  }

  readFrame() {
    const buf = this.buffer;
    if (buf.length < 2) return null;
    const opcode = buf[0] & 0x0f;
    let len = buf[1] & 0x7f;
    let offset = 2;
    if (len === 126) {
      if (buf.length < 4) return null;
      len = buf.readUInt16BE(2);
      offset = 4;
    } else if (len === 127) {
      if (buf.length < 10) return null;
      len = Number(buf.readBigUInt64BE(2));
      offset = 10;
    }
    if (buf.length < offset + len) return null;
    const payload = buf.subarray(offset, offset + len);
    this.buffer = buf.subarray(offset + len);
    if (opcode === 0x8) return { opcode, text: null };
    if (opcode === 0x9) {
      this.sendRaw(Buffer.from(payload), 0x8a);
      return this.readFrame();
    }
    return { opcode, text: payload.toString("utf8") };
  }

  sendRaw(payload, firstByte = 0x81) {
    const mask = randomBytes(4);
    const len = payload.length;
    let header;
    if (len < 126) {
      header = Buffer.from([firstByte, 0x80 | len]);
    } else if (len < 65536) {
      header = Buffer.alloc(4);
      header[0] = firstByte;
      header[1] = 0x80 | 126;
      header.writeUInt16BE(len, 2);
    } else {
      header = Buffer.alloc(10);
      header[0] = firstByte;
      header[1] = 0x80 | 127;
      header.writeBigUInt64BE(BigInt(len), 2);
    }
    const masked = Buffer.from(payload);
    for (let i = 0; i < masked.length; i += 1) masked[i] ^= mask[i % 4];
    this.socket.write(Buffer.concat([header, mask, masked]));
  }

  sendText(text) {
    this.sendRaw(Buffer.from(text, "utf8"));
  }

  nextMessage() {
    return new Promise((resolve) => {
      this.waiters.push(resolve);
      this.drain();
    });
  }
}

async function openWs(wsUrl) {
  const url = new URL(wsUrl);
  const socket = connect({ host: url.hostname, port: Number(url.port) }, () => {
    const key = randomBytes(16).toString("base64");
    socket.write(
      `GET ${url.pathname}${url.search} HTTP/1.1\r\n` +
        `Host: ${url.hostname}:${url.port}\r\n` +
        `Upgrade: websocket\r\nConnection: Upgrade\r\n` +
        `Sec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`,
    );
  });
  let ws = null;
  await new Promise((resolve, reject) => {
    let header = Buffer.alloc(0);
    const onData = (chunk) => {
      header = Buffer.concat([header, chunk]);
      if (header.includes("\r\n\r\n")) {
        socket.off("data", onData);
        if (!header.toString("latin1").startsWith("HTTP/1.1 101")) {
          reject(new Error("WebSocket 握手失败"));
          return;
        }
        const rest = header.subarray(header.indexOf("\r\n\r\n") + 4);
        ws = new MinimalWs(socket);
        if (rest.length > 0) ws.buffer = Buffer.from(rest);
        resolve(ws);
      }
    };
    socket.on("data", onData);
    socket.on("error", reject);
  });
  return ws;
}

function makeCdp(ws) {
  let id = 0;
  return async function call(method, params = {}) {
    id += 1;
    ws.sendText(JSON.stringify({ id, method, params }));
    for (;;) {
      const msg = await ws.nextMessage();
      if (msg.text === null) throw new Error("CDP 连接关闭");
      const parsed = JSON.parse(msg.text);
      if (parsed.id === id) {
        if (parsed.error) throw new Error(`${method}: ${parsed.error.message}`);
        return parsed.result ?? {};
      }
    }
  };
}

const EVAL_LAYOUT = `(() => {
  const de = document.documentElement;
  const btn = [...document.querySelectorAll('button')]
    .find((b) => b.textContent && (b.textContent.includes('验证指纹并上线') || b.textContent.includes('暂无会话')));
  const rect = btn ? btn.getBoundingClientRect() : null;
  return {
    scrollW: de.scrollWidth,
    clientW: de.clientWidth,
    innerW: window.innerWidth,
    bodyText: document.body.innerText.replace(/\\s+/g, ' ').slice(0, 40),
    btnInViewport: rect ? rect.right <= de.clientWidth + 1 && rect.left >= -1 && rect.width >= 44 : null,
  };
})()`;

async function main() {
  const edge = EDGE_CANDIDATES.find((p) => {
    try {
      accessSync(p);
      return true;
    } catch {
      return false;
    }
  });
  if (!edge) {
    console.error("✗ 未找到系统 Edge，无法执行真实浏览器渲染断言（降级，不视为通过）");
    process.exit(2);
  }

  // 直接运行 vite 的 JS 入口，避开 pnpm 外壳与 shell 转义问题。
  // 脚本自身位于 apps/mobile-web/scripts/，故 PWA 根为其上级目录。
  const scriptDir = dirname(fileURLToPath(import.meta.url));
  const pwaDir = resolve(scriptDir, "..");
  const preview = spawn(
    process.execPath,
    [join(pwaDir, "node_modules", "vite", "bin", "vite.js"), "preview", "--port", String(PREVIEW_PORT), "--strictPort"],
    { cwd: pwaDir, stdio: "ignore" },
  );

  let previewReady = false;
  for (let i = 0; i < 30; i += 1) {
    for (const probe of PROBE_URLS) {
      try {
        await httpJson(probe);
        previewReady = true;
        break;
      } catch {
        /* 换下一个探测地址 */
      }
    }
    if (previewReady) break;
    await sleep(1000);
  }
  if (!previewReady) {
    preview.kill();
    console.error("✗ vite preview 未能启动（先执行 pnpm --dir apps/mobile-web run build）");
    process.exit(2);
  }

  const browser = spawn(
    edge,
    [
      "--headless=new",
      `--remote-debugging-port=${CDP_PORT}`,
      `--user-data-dir=${mkdtempSync(join(tmpdir(), "soxu-vp-"))}`,
      "--no-first-run",
      "--disable-gpu",
      "--window-size=393,852",
      APP_URL,
    ],
    { stdio: "ignore" },
  );

  try {
    let target = null;
    for (let i = 0; i < 40; i += 1) {
      try {
        const list = await httpJson(`http://127.0.0.1:${CDP_PORT}/json/list`);
        target = list.find((t) => t.type === "page") ?? null;
        if (target) break;
      } catch {
        /* 等待 CDP 就绪 */
      }
      await sleep(500);
    }
    if (!target) {
      console.error("✗ Edge CDP 未就绪，无法完成真实渲染断言（降级，不视为通过）");
      process.exit(2);
    }

    const ws = await openWs(target.webSocketDebuggerUrl);
    const cdp = makeCdp(ws);

    let failed = 0;
    const lines = [];
    for (const [name, w, h, sizeClass] of MATRIX) {
      await cdp("Emulation.setDeviceMetricsOverride", {
        width: w,
        height: h,
        deviceScaleFactor: 1,
        mobile: true,
      });
      await cdp("Page.navigate", { url: APP_URL });
      await sleep(1200);
      const { result } = await cdp("Runtime.evaluate", {
        expression: EVAL_LAYOUT,
        returnByValue: true,
      });
      const v = result.value;
      const expectedClass = w < 640 ? "compact" : w < 1024 ? "regular" : "wide";
      const noOverflow = v.scrollW <= v.clientW;
      const classOk = expectedClass === sizeClass;
      const touchOk = v.btnInViewport !== false;
      const ok = noOverflow && classOk && touchOk;
      if (!ok) failed += 1;
      lines.push(
        `${ok ? "✓" : "✗"} ${name} ${w}x${h} [${sizeClass}] ` +
          `innerWidth=${v.innerW} scrollWidth=${v.scrollW} clientWidth=${v.clientW} ` +
          `主按钮视口内=${v.btnInViewport} 页面=${v.bodyText}`,
      );
    }

    console.log(lines.join("\n"));
    console.log(
      failed === 0
        ? `\n✓ 13 组视口真实浏览器渲染断言全部通过（pairing 引导页覆盖全矩阵；已认证 shell 需真实 daemon 另行验证）`
        : `\n✗ ${failed} 项断言失败`,
    );
    process.exitCode = failed === 0 ? 0 : 1;
  } finally {
    browser.kill();
    preview.kill();
  }
}

main().catch((err) => {
  console.error("✗ 断言执行失败:", err.message);
  process.exit(2);
});
