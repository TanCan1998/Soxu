import {
  AtSign,
  Check,
  ChevronDown,
  Compass,
  Eye,
  FileCode2,
  Folder,
  GitBranch,
  Image as ImageIcon,
  ListPlus,
  LockOpen,
  MessageSquareText,
  OctagonX,
  PencilLine,
  Plus,
  Quote,
  Rocket,
  ShieldCheck,
  Sparkles,
  Terminal,
  Upload,
  X,
  Zap,
} from "lucide-react";
import { useRef, useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  getAgentModels,
  getAgentProfile,
  getPermissionMeta,
  STATIC_AGENT_PROFILES,
  THINKING_LABELS,
} from "@/lib/agent-capabilities";
import { useI18n } from "@/lib/i18n";
import { formatShortcut } from "@/lib/useAdaptiveViewport";
import { cn } from "@/lib/utils";
import type {
  ComposerChip,
  ComposerChipKind,
  HandoffSelection,
  PermissionLevel,
  TurnBehavior,
} from "@/types";

function chipKindIcon(kind: ComposerChipKind, targetAgent?: string) {
  switch (kind) {
    case "handoff":
      return <AgentIcon agent={targetAgent ?? "codex"} className="h-3.5 w-3.5 shrink-0" />;
    case "quote":
      return <Quote className="h-3 w-3 shrink-0 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />;
    case "review":
      return <MessageSquareText className="h-3 w-3 shrink-0 text-[var(--soxu-semantic-warning)]" strokeWidth={1.75} />;
    case "folder":
      return <Folder className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />;
    case "diff":
      return <GitBranch className="h-3 w-3 shrink-0 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />;
    case "terminal":
      return <Terminal className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />;
    case "image":
      return <ImageIcon className="h-3 w-3 shrink-0 text-[var(--soxu-semantic-running)]" strokeWidth={1.75} />;
    case "skill":
      return <Sparkles className="h-3 w-3 shrink-0 text-[var(--soxu-semantic-warning)]" strokeWidth={1.75} />;
    case "file":
    default:
      return <FileCode2 className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />;
  }
}

/**
 * 按执行权限档位映射独立语义图标与识别色：
 * plan(规划/罗盘)、read-only(只读/眼睛)、ask(询问/盾牌确认)、auto-edit(自动编辑/画笔)、full-access(全权/开锁)、turbo(自主/火箭)。
 */
export function PermissionIcon({ level, className }: { level: PermissionLevel; className?: string }) {
  switch (level) {
    case "plan":
      return <Compass className={cn("h-3.5 w-3.5 text-[var(--soxu-semantic-running)] shrink-0", className)} strokeWidth={1.75} />;
    case "read-only":
      return <Eye className={cn("h-3.5 w-3.5 text-[var(--soxu-text-secondary)] shrink-0", className)} strokeWidth={1.75} />;
    case "auto-edit":
      return <PencilLine className={cn("h-3.5 w-3.5 text-[var(--soxu-semantic-warning)] shrink-0", className)} strokeWidth={1.75} />;
    case "full-access":
      return <LockOpen className={cn("h-3.5 w-3.5 text-[var(--soxu-semantic-danger)] shrink-0", className)} strokeWidth={1.75} />;
    case "turbo":
      return <Rocket className={cn("h-3.5 w-3.5 text-[var(--soxu-semantic-danger)] shrink-0", className)} strokeWidth={1.75} />;
    case "ask":
    default:
      return <ShieldCheck className={cn("h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0", className)} strokeWidth={1.75} />;
  }
}

const PERM_TONE_SURFACE: Record<"neutral" | "accent" | "warning" | "danger", string> = {
  neutral: "border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] hover:border-[var(--soxu-border-strong)]",
  accent: "border-[var(--soxu-semantic-accent)]/45 bg-[var(--soxu-semantic-accent-soft)] hover:border-[var(--soxu-semantic-accent)]/75",
  warning: "border-[var(--soxu-semantic-warning)]/45 bg-[var(--soxu-semantic-warning-soft)] hover:border-[var(--soxu-semantic-warning)]/75",
  danger: "border-[var(--soxu-semantic-danger)]/45 bg-[var(--soxu-semantic-danger-soft)] hover:border-[var(--soxu-semantic-danger)]/75",
};

const PERM_TONE_TEXT: Record<"neutral" | "accent" | "warning" | "danger", string> = {
  neutral: "text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)]",
  accent: "text-[var(--soxu-semantic-accent)]",
  warning: "text-[var(--soxu-semantic-warning)]",
  danger: "text-[var(--soxu-semantic-danger)]",
};

/**
 * 输入框内部上方挂载的结构化上下文与移交标签条（参考 Agy 桌面端）。
 */
export function ComposerChipBar({
  chips,
  onRemove,
  className,
}: {
  chips: ComposerChip[];
  onRemove?: (id: string) => void;
  className?: string;
}) {
  if (chips.length === 0) return null;

  return (
    <div className={cn("flex flex-wrap items-center gap-1.5 px-1 pt-0.5", className)}>
      {chips.map((chip) => {
        const isHandoff = chip.kind === "handoff";
        const isQuote = chip.kind === "quote";
        const isReview = chip.kind === "review";
        return (
          <span
            key={chip.id}
            className={cn(
              "inline-flex items-center gap-1.5 min-h-[26px] max-w-full rounded-lg border px-2 py-0.5 text-[11px] soxu-mono transition-colors",
              isHandoff
                ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/35 text-[var(--soxu-text-primary)]"
                : isQuote
                  ? "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)] border-l-2 border-l-[var(--soxu-semantic-accent)] text-[var(--soxu-text-primary)]"
                  : isReview
                    ? "bg-[var(--soxu-semantic-warning-soft)] border-[var(--soxu-semantic-warning-border)] text-[var(--soxu-text-primary)]"
                    : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)]",
            )}
          >
            {chipKindIcon(chip.kind, chip.targetAgent)}
            <span className="truncate max-w-[260px]">{chip.label}</span>
            {onRemove && (
              <button
                type="button"
                onClick={() => onRemove(chip.id)}
                aria-label={`移除标签 ${chip.label}`}
                className="soxu-hit-44 h-4 w-4 -mr-0.5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-colors shrink-0"
              >
                <X className="h-2.5 w-2.5" strokeWidth={1.75} />
              </button>
            )}
          </span>
        );
      })}
    </div>
  );
}

interface ComposerPlusMenuProps {
  hasWorkspace: boolean;
  onAddChip: (chip: Omit<ComposerChip, "id">) => void;
}

/**
 * 输入框左下角 `+` 号更多操作下拉菜单：
 * 支持一键追加代码文件、目录、Git 未提交变更、终端输出、本地附件图片与技能指令标签。
 */
export function ComposerPlusMenu({ hasWorkspace, onAddChip }: ComposerPlusMenuProps) {
  const { t } = useI18n();
  const fileInputRef = useRef<HTMLInputElement>(null);

  return (
    <>
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept="image/*,.rs,.ts,.tsx,.json,.md,.toml,.txt"
        className="hidden"
        onChange={(e) => {
          const picked = Array.from(e.target.files ?? []);
          picked.forEach((f) => {
            onAddChip({
              kind: f.type.startsWith("image/") ? "image" : "file",
              label: f.name,
            });
          });
          e.target.value = "";
        }}
      />

      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={t("composer.addMore")}
                className="soxu-hit-44 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-transparent hover:bg-[var(--soxu-interaction-hover)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
              >
                <Plus className="h-[18px] w-[18px]" strokeWidth={1.75} />
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>{t("composer.addMore")}</TooltipContent>
        </Tooltip>

        <DropdownMenuContent side="top" align="start" className="w-64">
          <DropdownMenuLabel>添加上下文与附件</DropdownMenuLabel>
          <DropdownMenuSeparator />

          {hasWorkspace && (
            <>
              <DropdownMenuItem
                onClick={() => onAddChip({ kind: "file", label: "crates/soxu-drivers/src/lib.rs:22-27" })}
              >
                <span className="flex items-center gap-2 min-w-0">
                  <FileCode2 className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                  <span className="truncate">引用源码片段 lib.rs:22-27</span>
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddChip({ kind: "file", label: "design-system/soxu/tokens.json" })}>
                <span className="flex items-center gap-2 min-w-0">
                  <FileCode2 className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                  <span className="truncate">引用设计令牌 tokens.json</span>
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddChip({ kind: "folder", label: "crates/soxu-core/" })}>
                <span className="flex items-center gap-2 min-w-0">
                  <Folder className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                  <span className="truncate">挂载目录 crates/soxu-core/</span>
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddChip({ kind: "diff", label: "未提交变更 (+142 -18)" })}>
                <span className="flex items-center gap-2 min-w-0">
                  <GitBranch className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                  <span className="truncate">附加工作树未提交 Diff</span>
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onAddChip({ kind: "terminal", label: "终端输出: cargo test" })}>
                <span className="flex items-center gap-2 min-w-0">
                  <Terminal className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                  <span className="truncate">附加最近终端日志 (cargo test)</span>
                </span>
              </DropdownMenuItem>
            </>
          )}

          <DropdownMenuItem onClick={() => fileInputRef.current?.click()}>
            <span className="flex items-center gap-2 min-w-0">
              <Upload className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-running)]" strokeWidth={1.75} />
              <span className="truncate">上传图片或本地文件...</span>
            </span>
          </DropdownMenuItem>

          <DropdownMenuSeparator />
          <DropdownMenuLabel>插入技能与工作流标签</DropdownMenuLabel>
          <DropdownMenuSeparator />

          <DropdownMenuItem onClick={() => onAddChip({ kind: "skill", label: "/plan 分步方案" })}>
            <span className="flex items-center gap-2 min-w-0">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-warning)]" strokeWidth={1.75} />
              <span className="truncate">/plan 制定执行方案</span>
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onAddChip({ kind: "skill", label: "/review 安全审阅" })}>
            <span className="flex items-center gap-2 min-w-0">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-warning)]" strokeWidth={1.75} />
              <span className="truncate">/review 安全红线审阅</span>
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onAddChip({ kind: "skill", label: "动作: check-all" })}>
            <span className="flex items-center gap-2 min-w-0">
              <Terminal className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-success)]" strokeWidth={1.75} />
              <span className="truncate">运行全仓门禁 check-all</span>
            </span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}

interface ComposerPermissionButtonProps {
  agentId: string;
  permLevel: PermissionLevel;
  onSelectPerm: (level: PermissionLevel, label: string) => void;
  onOpenSheet?: () => void;
  isMac?: boolean;
  showLabel?: boolean;
}

/**
 * 常驻输入舱操作权限按钮：
 * 绝不折叠进更多栏，支持紧凑纯图标模式或参考 Codex 风格的轻量无框文字+图标模式，悬浮提供档位说明与快捷键提示。
 */
export function ComposerPermissionButton({
  agentId,
  permLevel,
  onSelectPerm,
  onOpenSheet,
  isMac = false,
  showLabel = false,
}: ComposerPermissionButtonProps) {
  const { t } = useI18n();
  const profile = getAgentProfile(agentId);
  const activeMeta = getPermissionMeta(agentId, permLevel);
  const effectiveLevel = activeMeta.id;

  const triggerButton = showLabel ? (
    <button
      type="button"
      onClick={onOpenSheet}
      aria-label={t("composer.permTooltipTitle", { label: activeMeta.label })}
      className={cn(
        "soxu-hit-44 inline-flex items-center gap-1.5 h-8 px-2.5 rounded-lg border border-transparent bg-transparent hover:bg-[var(--soxu-interaction-hover)] cursor-pointer transition-colors text-xs font-medium shrink-0",
        PERM_TONE_TEXT[activeMeta.tone],
      )}
    >
      <PermissionIcon level={effectiveLevel} />
      <span className="whitespace-nowrap">{activeMeta.label}</span>
    </button>
  ) : (
    <button
      type="button"
      onClick={onOpenSheet}
      aria-label={t("composer.permTooltipTitle", { label: activeMeta.label })}
      className={cn(
        "soxu-hit-44 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full border cursor-pointer transition-colors",
        PERM_TONE_SURFACE[activeMeta.tone],
      )}
    >
      <PermissionIcon level={effectiveLevel} />
    </button>
  );

  const tooltip = (
    <TooltipContent side="top" className="max-w-[260px]">
      <div className="flex items-center justify-between gap-2 font-medium">
        <span>{t("composer.permTooltipTitle", { label: activeMeta.label })}</span>
        <kbd className="rounded bg-[var(--soxu-surface-control)] px-1 py-0.5 text-[10px] soxu-mono text-[var(--soxu-text-secondary)]">
          {formatShortcut("Alt+P", isMac)}
        </kbd>
      </div>
      <div className="mt-0.5 text-[10.5px] text-[var(--soxu-text-tertiary)] leading-snug">{activeMeta.desc}</div>
    </TooltipContent>
  );

  if (onOpenSheet) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{triggerButton}</TooltipTrigger>
        {tooltip}
      </Tooltip>
    );
  }

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{triggerButton}</DropdownMenuTrigger>
        </TooltipTrigger>
        {tooltip}
      </Tooltip>
      <DropdownMenuContent side="top" align="start" className="w-72">
        <DropdownMenuLabel>{profile.displayName} 支持的操作权限档位</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {profile.permissions.map((pm) => (
          <DropdownMenuItem
            key={pm.id}
            onClick={() => onSelectPerm(pm.id, pm.label)}
            className="items-start gap-2.5 py-2"
          >
            <PermissionIcon level={pm.id} className="mt-0.5" />
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2">
                <span className="font-semibold text-xs">{pm.label}</span>
                {effectiveLevel === pm.id && (
                  <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={2} />
                )}
              </div>
              <div className="text-[11px] text-[var(--soxu-text-tertiary)] leading-snug">{pm.desc}</div>
            </div>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

interface ComposerTurnBehaviorGroupProps {
  mode: TurnBehavior;
  supportsLiveSteer: boolean;
  onChange: (nextMode: TurnBehavior) => void;
  isMac?: boolean;
}

/**
 * 运行态投递行为纯图标分段组（热纠偏 / 排队 / 打断重发）：
 * 每个动作独立显示辨识度图标，不渲染文字标签，悬浮展示解释文案与真实快捷键提示。
 */
export function ComposerTurnBehaviorGroup({
  mode,
  supportsLiveSteer,
  onChange,
  isMac = false,
}: ComposerTurnBehaviorGroupProps) {
  const { t } = useI18n();

  const items: {
    id: TurnBehavior;
    label: string;
    desc: string;
    kbd: string;
    Icon: typeof Zap;
    activeClass: string;
  }[] = [
    ...(supportsLiveSteer
      ? [
          {
            id: "steer" as const,
            label: t("composer.modeSteer"),
            desc: t("composer.modeSteerDesc"),
            kbd: formatShortcut("Mod+Enter", isMac),
            Icon: Zap,
            activeClass:
              "bg-[var(--soxu-semantic-running-soft)] text-[var(--soxu-semantic-running)] border border-[var(--soxu-semantic-running)]/45",
          },
        ]
      : []),
    {
      id: "queue",
      label: t("composer.modeQueue"),
      desc: t("composer.modeQueueDesc"),
      kbd: formatShortcut("Mod+Shift+Enter", isMac),
      Icon: ListPlus,
      activeClass:
        "bg-[var(--soxu-semantic-warning-soft)] text-[var(--soxu-semantic-warning)] border border-[var(--soxu-semantic-warning)]/45",
    },
    {
      id: "interrupt",
      label: t("composer.modeInterrupt"),
      desc: t("composer.modeInterruptDesc"),
      kbd: formatShortcut("Alt+Enter", isMac),
      Icon: OctagonX,
      activeClass:
        "bg-[var(--soxu-semantic-danger-soft)] text-[var(--soxu-semantic-danger)] border border-[var(--soxu-semantic-danger)]/45",
    },
  ];

  return (
    <div
      role="group"
      aria-label="回合投递模式"
      className="inline-flex h-8 items-center gap-1 [@media(pointer:coarse)]:gap-1.5 rounded-full bg-[var(--soxu-surface-control)] border border-[var(--soxu-border-hairline)] p-0.5 shrink-0"
    >
      {items.map((item) => {
        const active = mode === item.id;
        const IconComp = item.Icon;
        return (
          <Tooltip key={item.id}>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => onChange(item.id)}
                aria-label={item.label}
                aria-pressed={active}
                className={cn(
                  "soxu-hit-44 inline-flex h-7 w-7 items-center justify-center rounded-full cursor-pointer transition-colors",
                  active
                    ? item.activeClass
                    : "border border-transparent text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-hover)]",
                )}
              >
                <IconComp className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-[260px]">
              <div className="flex items-center justify-between gap-2 font-medium">
                <span>{item.label}</span>
                <kbd className="rounded bg-[var(--soxu-surface-control)] px-1 py-0.5 text-[10px] soxu-mono text-[var(--soxu-text-secondary)]">
                  {item.kbd}
                </kbd>
              </div>
              <div className="mt-0.5 text-[10.5px] text-[var(--soxu-text-tertiary)] leading-snug">{item.desc}</div>
            </TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}

interface ComposerHandoffMenuProps {
  currentCli: string;
  currentModel: string;
  currentThinking: string;
  selectedHandoff?: HandoffSelection;
  iconOnly?: boolean;
  onSelectHandoff: (chipDraft: Omit<ComposerChip, "id">) => void;
  onClearHandoff: () => void;
  onOpenSheet?: () => void;
}

function resolveHandoffDraft(
  agentId: string,
  preferredModel?: string,
  preferredThinking?: string,
): { targetAgent: string; targetModel: string; targetThinking?: string; label: string } {
  const profile = getAgentProfile(agentId);
  const models = getAgentModels(agentId);
  const pickedModel = models.find((m) => m.id === preferredModel) ?? models[0];
  const nextThinking = profile.supportsThinking
    ? pickedModel.thinkingOptions.includes(preferredThinking ?? "")
      ? preferredThinking
      : (pickedModel.thinkingOptions[0] ?? "medium")
    : undefined;
  const shortModel = pickedModel.id.replace("group/", "").replace("auto/", "");
  const thinkText = nextThinking ? (THINKING_LABELS[nextThinking] ?? nextThinking) : "";
  const label = thinkText ? `@${agentId} / ${shortModel} (${thinkText})` : `@${agentId} / ${shortModel}`;

  return {
    targetAgent: agentId,
    targetModel: pickedModel.id,
    targetThinking: nextThinking,
    label,
  };
}

export function HandoffConfigPanel({
  currentCli,
  currentModel,
  currentThinking,
  selectedHandoff,
  onSelectHandoff,
  onClearHandoff,
  onDone,
}: ComposerHandoffMenuProps & { onDone: () => void }) {
  const { t } = useI18n();
  const availableClis = Object.values(STATIC_AGENT_PROFILES).map((p) => ({
    id: p.id,
    bin: p.id,
    ver: "",
    available: true,
  }));
  const defaultAgent =
    selectedHandoff?.targetAgent ?? availableClis.find((c) => c.id !== currentCli)?.id ?? currentCli;

  const [draftAgent, setDraftAgent] = useState(defaultAgent);
  const [draftModel, setDraftModel] = useState(selectedHandoff?.targetModel ?? currentModel);
  const [draftThinking, setDraftThinking] = useState(selectedHandoff?.targetThinking ?? currentThinking);

  const normalizedDraft = resolveHandoffDraft(draftAgent, draftModel, draftThinking);
  const draftProfile = getAgentProfile(normalizedDraft.targetAgent);
  const draftModels = getAgentModels(normalizedDraft.targetAgent);
  const draftModelMeta = draftModels.find((m) => m.id === normalizedDraft.targetModel) ?? draftModels[0];

  const emitSelection = (agentId: string, modelId?: string, thinkingId?: string) => {
    const resolved = resolveHandoffDraft(agentId, modelId, thinkingId);
    setDraftAgent(resolved.targetAgent);
    setDraftModel(resolved.targetModel);
    setDraftThinking(resolved.targetThinking ?? "");
    onSelectHandoff({
      kind: "handoff",
      label: resolved.label,
      targetAgent: resolved.targetAgent,
      targetModel: resolved.targetModel,
      targetThinking: resolved.targetThinking,
    });
  };

  return (
    <div className="space-y-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-[var(--soxu-text-primary)]">{t("composer.handoffChoose")}</span>
        {selectedHandoff && <Badge variant="handoff">@{selectedHandoff.targetAgent}</Badge>}
      </div>

      {/* 1. 选择目标 Agent（官方原版图标） */}
      <div className="grid grid-cols-2 gap-1.5">
        {availableClis.map((c) => {
          const prof = getAgentProfile(c.id);
          const active = normalizedDraft.targetAgent === c.id;
          return (
            <button
              key={c.id}
              type="button"
              onClick={() => emitSelection(c.id, draftModel, draftThinking)}
              className={cn(
                "min-h-[38px] flex items-center justify-between gap-1.5 rounded-lg border px-2.5 py-1.5 text-left text-[11.5px] cursor-pointer transition-colors",
                active
                  ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/55 text-[var(--soxu-text-primary)]"
                  : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-hairline)] text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-subtle)] hover:text-[var(--soxu-text-primary)]",
              )}
            >
              <span className="inline-flex items-center gap-1.5 min-w-0">
                <AgentIcon agent={c.id} className="h-3.5 w-3.5 shrink-0" />
                <span className="soxu-mono font-medium truncate">@{c.id}</span>
              </span>
              <span className="text-[10px] text-[var(--soxu-text-tertiary)] shrink-0">
                {c.id === currentCli ? "当前" : prof.displayName}
              </span>
            </button>
          );
        })}
      </div>

      <DropdownMenuSeparator className="my-1" />

      {/* 2. 直接指定目标模型 */}
      <div className="space-y-1">
        <div className="text-[11px] font-medium text-[var(--soxu-text-tertiary)]">
          {t("composer.handoffModelLabel")}
        </div>
        <div className="space-y-1 max-h-36 overflow-y-auto pr-0.5">
          {draftModels.map((m) => {
            const active = normalizedDraft.targetModel === m.id;
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => emitSelection(normalizedDraft.targetAgent, m.id, draftThinking)}
                className={cn(
                  "w-full min-h-[36px] flex items-center justify-between gap-2 rounded-lg border px-2.5 py-1.5 text-left text-[11px] cursor-pointer transition-colors",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
                    : "bg-transparent border-[var(--soxu-border-hairline)] text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)]",
                )}
              >
                <span className="soxu-mono truncate">{m.id.replace("group/", "").replace("auto/", "")}</span>
                <span className="inline-flex items-center gap-1 shrink-0">
                  <span className="text-[10px] text-[var(--soxu-text-tertiary)]">{m.tier}</span>
                  {active && <Check className="h-3 w-3 text-[var(--soxu-semantic-handoff)]" strokeWidth={2} />}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 3. 直接指定思考级别 */}
      <div className="space-y-1">
        <div className="text-[11px] font-medium text-[var(--soxu-text-tertiary)]">
          {t("composer.handoffThinkingLabel")}
        </div>
        {draftProfile.supportsThinking ? (
          <div className="flex flex-wrap gap-1.5">
            {draftModelMeta.thinkingOptions.map((lv) => {
              const active = normalizedDraft.targetThinking === lv;
              return (
                <button
                  key={lv}
                  type="button"
                  onClick={() => emitSelection(normalizedDraft.targetAgent, normalizedDraft.targetModel, lv)}
                  className={cn(
                    "min-h-[30px] px-3 rounded-full text-[11px] border cursor-pointer transition-colors",
                    active
                      ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/60 text-[var(--soxu-semantic-handoff)] font-medium"
                      : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)]",
                  )}
                >
                  {THINKING_LABELS[lv] ?? lv}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="text-[11px] text-[var(--soxu-text-tertiary)]">{t("composer.handoffNoThinking")}</div>
        )}
      </div>

      {/* 4. 底部确认与清除 */}
      <div className="flex items-center justify-between gap-2 pt-2 border-t border-[var(--soxu-border-hairline)]">
        {selectedHandoff ? (
          <Button
            size="sm"
            variant="ghost"
            className="h-8 px-2.5 text-[11px] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-semantic-danger)]"
            onClick={() => {
              onClearHandoff();
              onDone();
            }}
          >
            {t("composer.handoffClear")}
          </Button>
        ) : (
          <span />
        )}
        <Button
          size="sm"
          variant="default"
          className="h-8 px-3.5 text-[11px]"
          onClick={() => {
            emitSelection(normalizedDraft.targetAgent, normalizedDraft.targetModel, normalizedDraft.targetThinking);
            onDone();
          }}
        >
          {t("composer.handoffConfirm")}
        </Button>
      </div>
    </div>
  );
}

/**
 * 可选目标 Agent、模型与思考强度的移交面板：
 * 默认以纯净 `@` 图标按钮呈现（iconOnly=true），减少占用空间，激活态显示紫色柔底、边框与角标点，完整配置面板功能不变。
 */
export function ComposerHandoffMenu({
  currentCli,
  currentModel,
  currentThinking,
  selectedHandoff,
  iconOnly = true,
  onSelectHandoff,
  onClearHandoff,
  onOpenSheet,
}: ComposerHandoffMenuProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);

  const selectedSummary = selectedHandoff
    ? resolveHandoffDraft(selectedHandoff.targetAgent, selectedHandoff.targetModel, selectedHandoff.targetThinking).label
    : null;

  const triggerBtn = iconOnly ? (
    <button
      type="button"
      onClick={onOpenSheet}
      aria-pressed={Boolean(selectedHandoff)}
      aria-label={t("composer.handoffChoose")}
      className={cn(
        "soxu-hit-44 relative inline-flex items-center justify-center h-8 w-8 rounded-full border cursor-pointer transition-colors shrink-0",
        selectedHandoff
          ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/60 text-[var(--soxu-semantic-handoff)]"
          : "bg-transparent border-transparent text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-hover)]",
      )}
    >
      <AtSign className="h-4 w-4 shrink-0" strokeWidth={1.75} />
      {selectedHandoff && (
        <span className="absolute top-1 right-1 h-1.5 w-1.5 rounded-full bg-[var(--soxu-semantic-handoff)]" />
      )}
    </button>
  ) : (
    <button
      type="button"
      onClick={onOpenSheet}
      aria-pressed={Boolean(selectedHandoff)}
      aria-label={t("composer.handoffChoose")}
      className={cn(
        "soxu-hit-44 inline-flex items-center gap-1 h-8 px-2.5 rounded-full text-[11.5px] border cursor-pointer transition-colors shrink-0",
        selectedHandoff
          ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/60 text-[var(--soxu-semantic-handoff)] font-medium"
          : "bg-transparent border-transparent text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)]",
      )}
    >
      <AtSign className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
      <span>{t("composer.handoff")}</span>
    </button>
  );

  if (onOpenSheet) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{triggerBtn}</TooltipTrigger>
        <TooltipContent side="top" className="max-w-[260px]">
          <div className="font-medium">{t("composer.handoffChoose")}</div>
          {selectedSummary && (
            <div className="mt-0.5 text-[10.5px] soxu-mono text-[var(--soxu-semantic-handoff)]">{selectedSummary}</div>
          )}
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{triggerBtn}</DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-[260px]">
          <div className="font-medium">{t("composer.handoffChoose")}</div>
          {selectedSummary && (
            <div className="mt-0.5 text-[10.5px] soxu-mono text-[var(--soxu-semantic-handoff)]">{selectedSummary}</div>
          )}
        </TooltipContent>
      </Tooltip>

      <DropdownMenuContent side="top" align="start" className="w-[min(332px,calc(100vw-24px))] p-3">
        <HandoffConfigPanel
          currentCli={currentCli}
          currentModel={currentModel}
          currentThinking={currentThinking}
          selectedHandoff={selectedHandoff}
          onSelectHandoff={onSelectHandoff}
          onClearHandoff={onClearHandoff}
          onDone={() => setOpen(false)}
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

interface AgentModelConfigPanelProps {
  cli: string;
  model: string;
  thinking: string;
  onSelectCli: (nextCliId: string) => void;
  onSelectModel: (nextModelId: string) => void;
  onSelectThinking: (nextThinking: string) => void;
  onDone?: () => void;
}

/**
 * 代理驱动 + 模型 + 思考强度三合一配置面板（桌面下拉面板与移动端 Sheet 共用）。
 */
export function AgentModelConfigPanel({
  cli,
  model,
  thinking,
  onSelectCli,
  onSelectModel,
  onSelectThinking,
  onDone,
}: AgentModelConfigPanelProps) {
  const profile = getAgentProfile(cli);
  const models = getAgentModels(cli);
  const activeModelMeta = models.find((m) => m.id === model) ?? models[0];

  return (
    <div className="space-y-3">
      {/* 1. 原生代理驱动 */}
      <div className="space-y-1.5">
        <div className="text-[11px] font-medium text-[var(--soxu-text-tertiary)]">原生代理驱动</div>
        <div className="grid grid-cols-2 gap-1.5">
          {Object.values(STATIC_AGENT_PROFILES).map((p) => {
            const c = { id: p.id, bin: p.id, ver: "", available: true };
            const active = cli === c.id;
            return (
              <button
                key={c.id}
                type="button"
                disabled={!c.available}
                onClick={() => onSelectCli(c.id)}
                className={cn(
                  "min-h-[38px] flex items-center justify-between gap-1.5 rounded-lg border px-2.5 py-1.5 text-left text-[11.5px] transition-colors",
                  c.available ? "cursor-pointer" : "opacity-45 cursor-not-allowed",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
                    : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-hairline)] text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-subtle)] hover:text-[var(--soxu-text-primary)]",
                )}
              >
                <span className="inline-flex items-center gap-1.5 min-w-0">
                  <AgentIcon agent={c.id} className="h-3.5 w-3.5 shrink-0" />
                  <span className="soxu-mono font-medium truncate">{c.id}</span>
                </span>
                <span className="text-[10px] text-[var(--soxu-text-tertiary)] shrink-0">
                  {c.available ? c.ver || p.displayName : "未装"}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      <DropdownMenuSeparator className="my-1" />

      {/* 2. 模型选择 */}
      <div className="space-y-1.5">
        <div className="text-[11px] font-medium text-[var(--soxu-text-tertiary)]">{profile.displayName} 支持的模型</div>
        <div className="space-y-1 max-h-40 overflow-y-auto pr-0.5">
          {models.map((m) => {
            const active = model === m.id;
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => onSelectModel(m.id)}
                className={cn(
                  "w-full min-h-[36px] flex items-center justify-between gap-2 rounded-lg border px-2.5 py-1.5 text-left text-[11.5px] cursor-pointer transition-colors",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
                    : "bg-transparent border-[var(--soxu-border-hairline)] text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)]",
                )}
              >
                <span className="soxu-mono truncate">{m.id}</span>
                <span className="inline-flex items-center gap-1.5 shrink-0">
                  <span className="text-[10px] text-[var(--soxu-text-tertiary)]">{m.tier}</span>
                  {active && <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 3. 思考深度 */}
      <div className="space-y-1.5">
        <div className="text-[11px] font-medium text-[var(--soxu-text-tertiary)]">思考深度</div>
        {profile.supportsThinking ? (
          <div className="flex flex-wrap gap-1.5">
            {activeModelMeta.thinkingOptions.map((lv) => {
              const active = thinking === lv;
              return (
                <button
                  key={lv}
                  type="button"
                  onClick={() => onSelectThinking(lv)}
                  className={cn(
                    "min-h-[32px] px-3 rounded-full text-[11.5px] border cursor-pointer transition-colors",
                    active
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] font-semibold"
                      : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)]",
                  )}
                >
                  {THINKING_LABELS[lv] ?? lv}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="text-[11px] text-[var(--soxu-text-tertiary)]">当前代理驱动由模型自行决策，无需独立配置思考强度</div>
        )}
      </div>

      {onDone && (
        <div className="flex justify-end pt-2 border-t border-[var(--soxu-border-hairline)]">
          <Button size="sm" variant="default" className="h-8 px-4" onClick={onDone}>
            完成
          </Button>
        </div>
      )}
    </div>
  );
}

interface ComposerAgentModelPickerProps {
  cli: string;
  model: string;
  thinking: string;
  onSelectCli: (nextCliId: string) => void;
  onSelectModel: (nextModelId: string) => void;
  onSelectThinking: (nextThinking: string) => void;
  onOpenSheet?: () => void;
  className?: string;
}

/**
 * 参考 Codex 风格的无外框一体化「驱动图标 + 模型 · 思考强度」组合触发器：
 * 显著减少胶囊边框与重复文字，同时点击一次即可联动切换原生驱动、模型与思考深度。
 */
export function ComposerAgentModelPicker({
  cli,
  model,
  thinking,
  onSelectCli,
  onSelectModel,
  onSelectThinking,
  onOpenSheet,
  className,
}: ComposerAgentModelPickerProps) {
  const [open, setOpen] = useState(false);
  const profile = getAgentProfile(cli);
  const shortModel = model.replace("group/", "").replace("auto/", "");
  const thinkLabel = profile.supportsThinking ? (THINKING_LABELS[thinking] ?? thinking) : null;

  const triggerBtn = (
    <button
      type="button"
      onClick={onOpenSheet}
      aria-label={`当前驱动 ${cli}，模型 ${shortModel}${thinkLabel ? `，思考 ${thinkLabel}` : ""}`}
      className={cn(
        "soxu-hit-44 inline-flex items-center gap-1.5 h-8 px-2 rounded-lg border border-transparent bg-transparent hover:bg-[var(--soxu-interaction-hover)] min-w-0 max-w-full cursor-pointer text-[12px] transition-colors",
        className,
      )}
    >
      <AgentIcon agent={cli} className="h-3.5 w-3.5 shrink-0" />
      <span className="soxu-mono font-medium text-[var(--soxu-text-primary)] truncate">{shortModel}</span>
      {thinkLabel && <span className="text-[var(--soxu-text-tertiary)] whitespace-nowrap shrink-0">· {thinkLabel}</span>}
      <ChevronDown className="h-3 w-3 text-[var(--soxu-text-tertiary)] shrink-0" strokeWidth={1.75} />
    </button>
  );

  if (onOpenSheet) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{triggerBtn}</TooltipTrigger>
        <TooltipContent side="top">
          {profile.displayName} · {model}
          {thinkLabel ? ` · 思考 ${thinkLabel}` : ""}
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{triggerBtn}</DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          {profile.displayName} · {model}
          {thinkLabel ? ` · 思考 ${thinkLabel}` : ""}
        </TooltipContent>
      </Tooltip>

      <DropdownMenuContent side="top" align="end" className="w-[min(336px,calc(100vw-24px))] p-3">
        <AgentModelConfigPanel
          cli={cli}
          model={model}
          thinking={thinking}
          onSelectCli={onSelectCli}
          onSelectModel={onSelectModel}
          onSelectThinking={onSelectThinking}
          onDone={() => setOpen(false)}
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
