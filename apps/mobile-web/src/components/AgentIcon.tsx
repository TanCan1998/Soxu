import { cn } from "@/lib/utils";

export type CanonicalAgentId = "claude" | "codex" | "pi" | "agy" | "opencode" | "copilot" | "qwen";

const AGENT_BRAND_COLORS: Record<CanonicalAgentId, string> = {
  claude: "#e07a5f",
  codex: "#10a37f",
  pi: "#a78bfa",
  agy: "#60a5fa",
  opencode: "#2dd4bf",
  copilot: "#38bdf8",
  qwen: "#818cf8",
};

/**
 * 将类似 "codex"、"@codex"、"codex · group/gpt-6.2-sol" 或 "antigravity" 的复合标识归一化为标准代理 ID。
 */
export function normalizeAgentId(raw: string): CanonicalAgentId {
  const token = raw.trim().replace(/^@/, "").split(/[\s·/]+/)[0]?.toLowerCase() ?? "codex";
  if (token === "antigravity") return "agy";
  if (token in AGENT_BRAND_COLORS) return token as CanonicalAgentId;
  return "codex";
}

interface AgentIconProps {
  agent: string;
  className?: string;
  monochrome?: boolean;
}

/**
 * 7 大原生代理驱动专属矢量徽标：
 * 对齐 design-system/soxu/pages/04-magpie-and-cli-inspector.md 几何规范，各自具备独立高辨识度轮廓。
 */
export function AgentIcon({ agent, className, monochrome = false }: AgentIconProps) {
  const id = normalizeAgentId(agent);
  const colorStyle = monochrome ? undefined : { color: AGENT_BRAND_COLORS[id] };

  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={colorStyle}
      className={cn("shrink-0", className)}
    >
      {id === "claude" && (
        /* Claude：对称多芒火花星徽 */
        <>
          <path d="M12 2.75v4.5M12 16.75v4.5M2.75 12h4.5M16.75 12h4.5" />
          <path d="M5.46 5.46l3.18 3.18M15.36 15.36l3.18 3.18M18.54 5.46l-3.18 3.18M8.64 15.36l-3.18 3.18" />
          <circle cx="12" cy="12" r="2.2" fill="currentColor" stroke="none" />
        </>
      )}

      {id === "codex" && (
        /* Codex：六边形环结轮廓 */
        <>
          <path d="M12 2.6l7.8 4.5v9.8L12 21.4l-7.8-4.5V7.1L12 2.6z" />
          <path d="M12 7.2l4.2 2.4v4.8L12 16.8l-4.2-2.4V9.6L12 7.2z" />
          <path d="M12 2.6v4.6M19.8 7.1l-3.6 2.5M19.8 16.9l-3.6-2.5M12 21.4v-4.6M4.2 16.9l3.6-2.5M4.2 7.1l3.6 2.5" />
        </>
      )}

      {id === "pi" && (
        /* Pi：方框徽记与几何希腊字母 π */
        <>
          <rect x="3.5" y="3.5" width="17" height="17" rx="4.5" />
          <path d="M7.25 9.25h9.5" />
          <path d="M10 9.25v6.75c0 .7-.45 1.25-1.15 1.25" />
          <path d="M14.25 9.25v5.65c0 1.05.5 1.6 1.35 1.6.35 0 .65-.1.9-.25" />
        </>
      )}

      {id === "agy" && (
        /* Agy (Antigravity)：流线升空 A 形拱翼 + 顶部悬浮星芒 */
        <>
          <path d="M5.2 19.5L11 6.2c.4-.9 1.6-.9 2 0l5.8 13.3" />
          <path d="M7.6 14.8c2.7-1.6 6.1-1.6 8.8 0" />
          <circle cx="12" cy="3.2" r="1.35" fill="currentColor" stroke="none" />
        </>
      )}

      {id === "opencode" && (
        /* OpenCode：嵌套开口几何终端方框 */
        <>
          <path d="M9.5 4.2H6A2.2 2.2 0 0 0 3.8 6.4v11.2A2.2 2.2 0 0 0 6 19.8h12a2.2 2.2 0 0 0 2.2-2.2V6.4A2.2 2.2 0 0 0 18 4.2h-3.5" />
          <path d="M8.5 10.2l2.3 2.1-2.3 2.1M12.5 15h3.5" />
        </>
      )}

      {id === "copilot" && (
        /* Copilot：圆弧飞行护目镜与侧翼 */
        <>
          <path d="M6.5 9.2a5.5 5.5 0 0 1 11 0v2.1a5.5 5.5 0 0 1-11 0V9.2z" />
          <path d="M4 11.2c-.8.4-1.2 1.2-1.2 2.2 0 1.4 1 2.4 2.5 2.6" />
          <path d="M20 11.2c.8.4 1.2 1.2 1.2 2.2 0 1.4-1 2.4-2.5 2.6" />
          <path d="M9.5 10.5v2M14.5 10.5v2" />
          <path d="M8.5 19.2c1.1-.9 2.3-1.2 3.5-1.2s2.4.3 3.5 1.2" />
        </>
      )}

      {id === "qwen" && (
        /* Qwen：双角星交织环形几何体 */
        <>
          <polygon points="12,3 19.5,16 4.5,16" />
          <polygon points="12,21 4.5,8 19.5,8" />
          <circle cx="12" cy="12" r="1.8" fill="currentColor" stroke="none" />
        </>
      )}
    </svg>
  );
}
