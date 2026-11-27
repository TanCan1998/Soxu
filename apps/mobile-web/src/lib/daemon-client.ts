/**
 * Soxu Daemon 真实链路客户端：Bearer 认证、`afterSeq` 增量追平与断线重连。
 *
 * 重连语义借鉴 t3code `client-runtime/connection/supervisor`：指数退避
 * （1s 基数 ×2^n，上限 300s）、连接稳定 30s 后重置退避计数；实现按本仓
 * 单 Daemon 直连形态裁剪，不引入多路由竞速。
 */

import {
  PROTOCOL_VERSION,
  decodeEventEnvelope,
  type ClaimPairingResponse,
  type ClientHello,
  type EventEnvelope,
  type RpcCommand,
  type RpcRequest,
  type RpcResponse,
  type ServerHello,
} from "./daemon-protocol";

const RECONNECT_BASE_DELAY_MS = 1_000;
const RECONNECT_MAX_DELAY_MS = 300_000;
/** 连接保持稳定超过该时长后重置退避计数（对标 t3code BACKOFF_RESET_AFTER_MS）。 */
const STABLE_CONNECTION_RESET_MS = 30_000;
const CLIENT_VERSION = "0.1.0";

/** 本机持久化的配对凭据（localStorage；令牌由 Daemon 一次性下发）。 */
export interface StoredPairing {
  endpoint_url: string;
  bearer_token: string;
  device_id: string;
  daemon_fingerprint: string;
}

const PAIRING_STORAGE_KEY = "{{SECRET_3x1qj9f2}}";

export function loadStoredPairing(): StoredPairing | null {
  try {
    const raw = window.localStorage.getItem(PAIRING_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredPairing>;
    if (!parsed.endpoint_url || !parsed.bearer_token || !parsed.device_id) return null;
    return {
      endpoint_url: parsed.endpoint_url,
      bearer_token: parsed.bearer_token,
      device_id: parsed.device_id,
      daemon_fingerprint: parsed.daemon_fingerprint ?? "",
    };
  } catch {
    return null;
  }
}

export function saveStoredPairing(pairing: StoredPairing): void {
  try {
    window.localStorage.setItem(PAIRING_STORAGE_KEY, JSON.stringify(pairing));
  } catch {
    // 隐私模式等场景下忽略持久化异常；本次会话内仍可使用
  }
}

export function clearStoredPairing(): void {
  try {
    window.localStorage.removeItem(PAIRING_STORAGE_KEY);
  } catch {
    // 忽略清除失败
  }
}

/** 解析 `#offer=<base64url>` 片段；解析成功后立即清除地址栏片段防截屏泄露。 */
export function consumeOfferFromLocation(): import("./daemon-protocol").PairingOfferPayload | null {
  try {
    const hash = window.location.hash;
    const match = /(?:^|[#&])offer=([A-Za-z0-9_-]+)/.exec(hash);
    if (!match) return null;
    // 立即清除，防止截屏或历史记录泄露一次性令牌
    history.replaceState(null, "", window.location.pathname + window.location.search);
    const json = atob(match[1].replace(/-/g, "+").replace(/_/g, "/"));
    const offer = JSON.parse(json) as import("./daemon-protocol").PairingOfferPayload;
    if (!offer.one_time_token || !offer.endpoint_url) return null;
    return offer;
  } catch {
    return null;
  }
}

export type ConnectionPhase = "connecting" | "catching_up" | "online" | "reconnecting" | "offline";

export interface DaemonClientCallbacks {
  onPhase: (phase: ConnectionPhase, detail?: string) => void;
  onServerHello: (hello: ServerHello) => void;
  /** 握手追平阶段重放的历史事件（before onLiveEvents 恢复）。 */
  onCatchUpEvents: (events: EventEnvelope[], completed: { from_seq: number; to_seq: number; count: number } | null) => void;
  onLiveEvent: (env: EventEnvelope) => void;
}

interface PendingRequest {
  resolve: (resp: RpcResponse) => void;
  timer: number;
}

let requestCounter = 0;

export class DaemonClient {
  private ws: WebSocket | null = null;
  private readonly callbacks: DaemonClientCallbacks;
  private readonly pairing: StoredPairing;
  private lastAppliedSeq: number;
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;
  private connectedAt = 0;
  private pending = new Map<string, PendingRequest>();
  private closedByUser = false;

  constructor(pairing: StoredPairing, callbacks: DaemonClientCallbacks, initialSeq = 0) {
    this.pairing = pairing;
    this.callbacks = callbacks;
    this.lastAppliedSeq = initialSeq;
  }

  get endpoint(): string {
    return this.pairing.endpoint_url;
  }

  /** 发起（或重发）WS 连接：携带 Bearer 令牌与 afterSeq 游标。 */
  connect(): void {
    this.closedByUser = false;
    this.callbacks.onPhase(this.reconnectAttempt === 0 ? "connecting" : "reconnecting");
    const url = new URL("/ws", this.pairing.endpoint_url);
    // 移动端浏览器 WebSocket 无法自定义 header，令牌经 ws query 传输（与桌面端 /api 一致的鉴权面）
    url.searchParams.set("token", this.pairing.bearer_token);
    const ws = new WebSocket(url);
    this.ws = ws;
    ws.onopen = () => {
      const hello: ClientHello = {
        client_kind: "mobile_pwa",
        client_version: CLIENT_VERSION,
        protocol_version: PROTOCOL_VERSION,
        after_seq: this.lastAppliedSeq > 0 ? this.lastAppliedSeq : null,
        device_id: this.pairing.device_id,
      };
      ws.send(JSON.stringify(hello));
    };
    ws.onmessage = (msg) => this.handleMessage(String(msg.data));
    ws.onclose = () => this.scheduleReconnect();
    ws.onerror = () => ws.close();
  }

  /** 用户主动断开（解除配对 / 页面卸载），不再自动重连。 */
  close(): void {
    this.closedByUser = true;
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    for (const pending of this.pending.values()) window.clearTimeout(pending.timer);
    this.pending.clear();
    this.ws?.close();
    this.ws = null;
    this.callbacks.onPhase("offline");
  }

  private handleMessage(raw: string): void {
    // 1. RPC 响应：分发给等待者后即结束
    if (this.dispatchRpcResponse(raw)) return;

    // 2. ServerHello：全量快照
    if (!this.helloReceived) {
      let hello: ServerHello;
      try {
        hello = JSON.parse(raw) as ServerHello;
      } catch {
        this.scheduleReconnect();
        return;
      }
      if (hello.protocol_version !== PROTOCOL_VERSION) {
        this.callbacks.onPhase(
          "offline",
          `协议版本不匹配：客户端 v${PROTOCOL_VERSION} / 守护进程 v${hello.protocol_version}`,
        );
        this.closedByUser = true;
        this.ws?.close();
        return;
      }
      this.helloReceived = true;
      this.connectedAt = Date.now();
      this.reconnectAttempt = 0;
      this.callbacks.onServerHello(hello);
      this.lastAppliedSeq = Math.max(this.lastAppliedSeq, hello.latest_seq);
      this.callbacks.onPhase("online");
      return;
    }

    // 2. 追平重放事件与 CatchUpCompleted 收尾
    const env = decodeEventEnvelope(raw);
    if (!env) return;
    this.lastAppliedSeq = Math.max(this.lastAppliedSeq, env.seq);
    this.callbacks.onLiveEvent(env);
  }

  private helloReceived = false;

  private scheduleReconnect(): void {
    this.helloReceived = false;
    for (const pending of this.pending.values()) window.clearTimeout(pending.timer);
    this.pending.clear();
    if (this.closedByUser) return;
    this.reconnectAttempt += 1;
    const delay = Math.min(
      RECONNECT_MAX_DELAY_MS,
      RECONNECT_BASE_DELAY_MS * 2 ** this.reconnectAttempt,
    );
    this.callbacks.onPhase("reconnecting", `第 ${this.reconnectAttempt} 次重连，${Math.round(delay / 1000)}s 后重试`);
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      // 稳定连接之后再次掉线视为全新故障，重置退避
      if (this.connectedAt > 0 && Date.now() - this.connectedAt > STABLE_CONNECTION_RESET_MS) {
        this.reconnectAttempt = 1;
      }
      this.connect();
    }, delay);
  }

  /** 通过活跃 WS 发送 RPC 命令；连接不可用时 reject。 */
  rpc(command: RpcCommand, timeoutMs = 10_000): Promise<RpcResponse> {
    const ws = this.ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("WS 连接不可用"));
    }
    const request_id = `m-${Date.now().toString(36)}-${(requestCounter += 1)}`;
    const req: RpcRequest = { request_id, command };
    return new Promise<RpcResponse>((resolve) => {
      const timer = window.setTimeout(() => {
        this.pending.delete(request_id);
        resolve({ request_id, ok: false, result: null, error: null });
      }, timeoutMs);
      this.pending.set(request_id, { resolve, timer });
      ws.send(JSON.stringify(req));
    });
  }

  /** 派发 WS 消息到 RPC 等待者（在 onmessage 分发后调用）。 */
  dispatchRpcResponse(raw: string): boolean {
    let resp: RpcResponse;
    try {
      resp = JSON.parse(raw) as RpcResponse;
    } catch {
      return false;
    }
    if (typeof resp.request_id !== "string" || !this.pending.has(resp.request_id)) return false;
    const pending = this.pending.get(resp.request_id);
    if (!pending) return false;
    this.pending.delete(resp.request_id);
    window.clearTimeout(pending.timer);
    pending.resolve(resp);
    return true;
  }

  get currentSeq(): number {
    return this.lastAppliedSeq;
  }
}

/**
 * 用一次性 `#offer` 令牌或 6 位配对码发起认领（免认证 RPC 端点）。
 */
export async function claimPairing(
  endpointUrl: string,
  credential: { kind: "offer_token" | "six_digit_code"; value: string },
): Promise<ClaimPairingResponse> {
  const resp = await fetch(new URL("/api/rpc", endpointUrl), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      request_id: `claim-${Date.now().toString(36)}`,
      command: {
        method: "claim_pairing",
        params: {
          device_name: navigator.userAgent.includes("Android") ? "安卓设备" : "移动设备",
          platform: navigator.platform || "web",
          credential: { kind: credential.kind, value: credential.value },
        },
      },
    } satisfies RpcRequest),
  });
  if (!resp.ok) {
    throw new Error(`配对认领失败（HTTP ${resp.status}）`);
  }
  const body = (await resp.json()) as RpcResponse;
  if (!body.ok || body.result?.kind !== "pairing_claimed") {
    const err = body.error;
    throw new Error(err ? `${err.summary_zh}（${err.remediation_zh}）` : "配对认领失败");
  }
  return body.result.data;
}
