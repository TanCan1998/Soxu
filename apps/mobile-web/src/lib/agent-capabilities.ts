import { MODEL_OPTIONS } from "@/mock-data";
import type { PermissionLevel, TurnBehavior } from "@/types";

export interface PermissionMeta {
  id: PermissionLevel;
  label: string;
  desc: string;
  tone: "neutral" | "accent" | "warning" | "danger";
}

export interface AgentCapabilityProfile {
  id: string;
  displayName: string;
  supportedModels: string[];
  permissions: PermissionMeta[];
  supportsThinking: boolean;
  supportsLiveSteer: boolean;
}

export const THINKING_LABELS: Record<string, string> = {
  low: "低",
  medium: "中",
  high: "高",
  xhigh: "极高",
  max: "最高",
};

export const MODEL_CONTEXT_LIMITS: Record<string, number> = {
  "group/gpt-6.2-astra": 256000,
  "group/claude-opus-5.5": 200000,
  "gemini-4-carbon": 1000000,
  "group/gpt-6.2-sol": 200000,
  "group/claude-sonnet-5.5": 200000,
  "auto/deepseek-v4.1-flash-1017": 128000,
  "auto/gemini-4-hydrogen": 256000,
};

export const AGENT_CAPABILITIES: Record<string, AgentCapabilityProfile> = {
  agy: {
    id: "agy",
    displayName: "Agy",
    supportedModels: [
      "gemini-4-carbon",
      "group/claude-opus-5.5",
      "group/claude-sonnet-5.5",
      "group/gpt-6.2-astra",
      "auto/gemini-4-hydrogen",
    ],
    permissions: [
      { id: "plan", label: "规划确认", desc: "先产出完整执行方案再人工拍板", tone: "neutral" },
      { id: "ask", label: "询问执行", desc: "写文件与执行命令均逐项确认", tone: "accent" },
      { id: "auto-edit", label: "自动编辑", desc: "工作区内编辑放行，终端命令需确认", tone: "warning" },
      { id: "turbo", label: "自主运行", desc: "隔离工作树内自动推进，仅高危拦截", tone: "danger" },
    ],
    supportsThinking: true,
    supportsLiveSteer: false,
  },
  codex: {
    id: "codex",
    displayName: "Codex",
    supportedModels: [
      "group/gpt-6.2-astra",
      "group/gpt-6.2-sol",
      "group/claude-sonnet-5.5",
      "auto/deepseek-v4.1-flash-1017",
    ],
    permissions: [
      { id: "read-only", label: "只读分析", desc: "禁止写入任何工作区文件或执行改动命令", tone: "neutral" },
      { id: "ask", label: "询问确认", desc: "文件写入或命令执行前请求人工确认", tone: "accent" },
      { id: "auto-edit", label: "自动写入", desc: "工作区内修改自动写入，危险命令仍确认", tone: "warning" },
      { id: "full-access", label: "完全授权", desc: "当前工作树全权自主运行（受红线保护）", tone: "danger" },
    ],
    supportsThinking: true,
    supportsLiveSteer: true,
  },
  claude: {
    id: "claude",
    displayName: "Claude",
    supportedModels: [
      "group/claude-opus-5.5",
      "group/claude-sonnet-5.5",
      "group/gpt-6.2-sol",
      "auto/deepseek-v4.1-flash-1017",
    ],
    permissions: [
      { id: "plan", label: "只读规划", desc: "只读取和制定方案，不执行磁盘写入", tone: "neutral" },
      { id: "ask", label: "每次询问", desc: "严格遵从交互式写入与执行确认", tone: "accent" },
      { id: "auto-edit", label: "自动接受编辑", desc: "代码修改自动应用，命令执行需审批", tone: "warning" },
      { id: "full-access", label: "跳过常规确认", desc: "隔离环境自主推进（高危命令仍拦截）", tone: "danger" },
    ],
    supportsThinking: true,
    supportsLiveSteer: true,
  },
  pi: {
    id: "pi",
    displayName: "Pi",
    supportedModels: [
      "group/gpt-6.2-sol",
      "group/claude-sonnet-5.5",
      "auto/deepseek-v4.1-flash-1017",
      "auto/gemini-4-hydrogen",
    ],
    permissions: [
      { id: "ask", label: "每次询问", desc: "执行关键写操作前请求人工授权", tone: "accent" },
      { id: "auto-edit", label: "自动编辑", desc: "自动写入代码变更并审批外围命令", tone: "warning" },
      { id: "full-access", label: "工作树全权", desc: "多步并行代理全自动编排运行", tone: "danger" },
    ],
    supportsThinking: true,
    supportsLiveSteer: true,
  },
  opencode: {
    id: "opencode",
    displayName: "OpenCode",
    supportedModels: [
      "group/gpt-6.2-sol",
      "group/claude-sonnet-5.5",
      "auto/deepseek-v4.1-flash-1017",
    ],
    permissions: [
      { id: "plan", label: "仅规划分析", desc: "仅可查阅代码与输出结构化规划", tone: "neutral" },
      { id: "ask", label: "每次询问", desc: "修改前需确认", tone: "accent" },
      { id: "auto-edit", label: "自动编辑", desc: "仅自动应用局部代码补丁", tone: "warning" },
    ],
    supportsThinking: true,
    supportsLiveSteer: false,
  },
  copilot: {
    id: "copilot",
    displayName: "Copilot",
    supportedModels: ["group/gpt-6.2-sol", "group/claude-sonnet-5.5"],
    permissions: [
      { id: "ask", label: "每次询问", desc: "默认询问确认模式", tone: "accent" },
      { id: "auto-edit", label: "自动编辑", desc: "工作区局部写入免确认", tone: "warning" },
    ],
    supportsThinking: false,
    supportsLiveSteer: false,
  },
  qwen: {
    id: "qwen",
    displayName: "Qwen",
    supportedModels: ["auto/deepseek-v4.1-flash-1017", "group/gpt-6.2-sol"],
    permissions: [
      { id: "read-only", label: "只读分析", desc: "只读检索模式", tone: "neutral" },
      { id: "ask", label: "每次询问", desc: "交互式授权", tone: "accent" },
    ],
    supportsThinking: false,
    supportsLiveSteer: false,
  },
};

export function getAgentProfile(agentId: string): AgentCapabilityProfile {
  return AGENT_CAPABILITIES[agentId] ?? AGENT_CAPABILITIES.codex;
}

export function getPermissionMeta(agentId: string, perm: PermissionLevel): PermissionMeta {
  const profile = getAgentProfile(agentId);
  return profile.permissions.find((p) => p.id === perm) ?? profile.permissions[0];
}

export function getAgentModels(agentId: string) {
  const profile = getAgentProfile(agentId);
  const filtered = MODEL_OPTIONS.filter((m) => profile.supportedModels.includes(m.id));
  return filtered.length > 0 ? filtered : MODEL_OPTIONS;
}

export function reconcileAgentCapabilities(params: {
  agentId: string;
  currentModel: string;
  currentPerm: PermissionLevel;
  currentThinking: string;
  currentBehavior: TurnBehavior;
}): {
  model: string;
  perm: PermissionLevel;
  thinking: string;
  behavior: TurnBehavior;
  notes: string[];
} {
  const profile = getAgentProfile(params.agentId);
  const models = getAgentModels(params.agentId);
  const notes: string[] = [];

  let nextModel = params.currentModel;
  if (!models.some((m) => m.id === nextModel)) {
    nextModel = models[0].id;
    notes.push(`已将模型切换为 ${profile.displayName} 支持的 ${nextModel}`);
  }

  let nextPerm = params.currentPerm;
  if (!profile.permissions.some((p) => p.id === nextPerm)) {
    const fallbackPerm = profile.permissions.find((p) => p.id === "ask") ?? profile.permissions[0];
    nextPerm = fallbackPerm.id;
    notes.push(`权限已自适应调整为「${fallbackPerm.label}」`);
  }

  const modelMeta = MODEL_OPTIONS.find((m) => m.id === nextModel) ?? models[0];
  let nextThinking = params.currentThinking;
  if (profile.supportsThinking && !modelMeta.thinkingOptions.includes(nextThinking)) {
    nextThinking = modelMeta.thinkingOptions[0] ?? "medium";
    notes.push(`思考强度调整为「${THINKING_LABELS[nextThinking] ?? nextThinking}」`);
  }

  let nextBehavior = params.currentBehavior;
  if (!profile.supportsLiveSteer && nextBehavior === "steer") {
    nextBehavior = "queue";
    notes.push(`${profile.displayName} 暂不支持热纠偏，投递已调整为排队`);
  }

  return {
    model: nextModel,
    perm: nextPerm,
    thinking: nextThinking,
    behavior: nextBehavior,
    notes,
  };
}
