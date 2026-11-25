import {
  AtSign,
  Check,
  ChevronDown,
  FileCode2,
  Folder,
  GitBranch,
  Image as ImageIcon,
  Plus,
  Sparkles,
  Terminal,
  Upload,
  X,
} from "lucide-react";
import { useRef } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { getAgentProfile } from "@/lib/agent-capabilities";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { CLI_OPTIONS } from "@/mock-data";
import type { ComposerChip, ComposerChipKind } from "@/types";

export function chipKindIcon(kind: ComposerChipKind, targetAgent?: string) {
  switch (kind) {
    case "handoff":
      return <AgentIcon agent={targetAgent ?? "codex"} className="h-3.5 w-3.5 shrink-0" />;
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
        return (
          <span
            key={chip.id}
            className={cn(
              "inline-flex items-center gap-1.5 h-[26px] max-w-full rounded-lg border px-2 text-[11px] soxu-mono transition-colors",
              isHandoff
                ? "bg-[var(--soxu-semantic-handoff-soft)] border-[var(--soxu-semantic-handoff)]/35 text-[var(--soxu-text-primary)]"
                : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)]",
            )}
          >
            {chipKindIcon(chip.kind, chip.targetAgent)}
            <span className="truncate max-w-[220px]">{chip.label}</span>
            {onRemove && (
              <button
                type="button"
                onClick={() => onRemove(chip.id)}
                aria-label={`移除标签 ${chip.label}`}
                className="h-4 w-4 -mr-0.5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-colors shrink-0"
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
                className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[var(--soxu-surface-control)] hover:bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
              >
                <Plus className="h-4 w-4" strokeWidth={1.75} />
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

interface ComposerHandoffMenuProps {
  currentCli: string;
  selectedTargetAgent?: string;
  onSelectTarget: (targetAgent: string) => void;
}

/**
 * 可选目标 Agent 的移交下拉菜单：
 * 展示全部可用原生驱动及专属图标，选中后作为移交标签加入输入框顶部。
 */
export function ComposerHandoffMenu({ currentCli, selectedTargetAgent, onSelectTarget }: ComposerHandoffMenuProps) {
  const { t } = useI18n();

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className={cn(
                "inline-flex items-center gap-1 h-7 px-2 rounded-full text-[11px] cursor-pointer transition-colors",
                selectedTargetAgent
                  ? "bg-[var(--soxu-semantic-handoff-soft)] border border-[var(--soxu-semantic-handoff)]/35 text-[var(--soxu-text-primary)]"
                  : "text-[var(--soxu-semantic-handoff)] hover:bg-[var(--soxu-interaction-hover)]",
              )}
              aria-label={t("composer.handoffChoose")}
            >
              {selectedTargetAgent ? (
                <>
                  <AgentIcon agent={selectedTargetAgent} className="h-3.5 w-3.5 shrink-0" />
                  <span className="soxu-mono">@{selectedTargetAgent}</span>
                </>
              ) : (
                <>
                  <AtSign className="h-3 w-3 shrink-0" strokeWidth={1.75} />
                  <span>{t("composer.handoff")}</span>
                </>
              )}
              <ChevronDown className="h-3 w-3 opacity-75 shrink-0" strokeWidth={1.75} />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{t("composer.handoffChoose")}</TooltipContent>
      </Tooltip>

      <DropdownMenuContent side="top" align="start" className="w-60">
        <DropdownMenuLabel>{t("composer.handoffChoose")}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {CLI_OPTIONS.filter((c) => c.available).map((c) => {
          const isCurrent = c.id === currentCli;
          const isPicked = c.id === selectedTargetAgent;
          const prof = getAgentProfile(c.id);
          return (
            <DropdownMenuItem key={c.id} onClick={() => onSelectTarget(c.id)}>
              <span className="inline-flex items-center gap-2 min-w-0">
                <AgentIcon agent={c.id} className="h-4 w-4 shrink-0" />
                <span className="soxu-mono font-medium">@{c.id}</span>
                <span className="text-[11px] text-[var(--soxu-text-tertiary)] truncate">{prof.displayName}</span>
              </span>
              {isPicked ? (
                <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)] shrink-0" strokeWidth={2} />
              ) : isCurrent ? (
                <Badge variant="neutral">当前</Badge>
              ) : (
                <Badge variant="handoff">v{c.ver}</Badge>
              )}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
