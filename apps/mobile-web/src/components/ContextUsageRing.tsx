import { Minimize2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { MODEL_CONTEXT_LIMITS } from "@/lib/agent-capabilities";
import { cn } from "@/lib/utils";

interface ContextUsageRingProps {
  model: string;
  usedTokens: number;
  onCompact?: () => void;
  onOpenChangeLocked?: (open: boolean) => void;
  onOpenSheet?: () => void;
  compact?: boolean;
  className?: string;
}

function formatTokenCount(n: number): string {
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

export function ContextUsagePanel({
  model,
  usedTokens,
  onCompact,
  onClose,
}: {
  model: string;
  usedTokens: number;
  onCompact?: () => void;
  onClose?: () => void;
}) {
  const limit = MODEL_CONTEXT_LIMITS[model] ?? 200000;
  const rawPct = Math.min(100, Math.max(0.1, (usedTokens / limit) * 100));
  const pctLabel = rawPct >= 99.95 ? "100%" : `${rawPct.toFixed(1)}%`;

  const ringColor =
    rawPct >= 88
      ? "var(--soxu-semantic-danger)"
      : rawPct >= 65
        ? "var(--soxu-semantic-warning)"
        : "var(--soxu-semantic-success)";

  const sysTokens = Math.round(usedTokens * 0.14);
  const historyTokens = Math.round(usedTokens * 0.44);
  const toolTokens = Math.round(usedTokens * 0.24);
  const codeTokens = Math.max(0, usedTokens - sysTokens - historyTokens - toolTokens);

  const segments = [
    { key: "sys", label: "系统规则", tokens: sysTokens, color: "var(--soxu-semantic-accent)" },
    { key: "hist", label: "对话历史", tokens: historyTokens, color: "var(--soxu-semantic-running)" },
    { key: "tool", label: "工具输出", tokens: toolTokens, color: "var(--soxu-semantic-warning)" },
    { key: "code", label: "代码引用", tokens: codeTokens, color: "var(--soxu-semantic-handoff)" },
  ];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between text-xs">
        <span className="font-semibold text-[var(--soxu-text-primary)]">上下文窗口用量</span>
        <span className="soxu-mono soxu-tabular font-medium" style={{ color: ringColor }}>
          {pctLabel} ({formatTokenCount(usedTokens)} / {formatTokenCount(limit)})
        </span>
      </div>

      {/* 多段语义构成条（吸收 Oryn 构成条与 Soxu 三档状态） */}
      <div className="h-2 w-full rounded-full bg-[var(--soxu-surface-control)] overflow-hidden flex gap-px p-[1px]">
        {segments.map((seg) => {
          const share = Math.max(2, (seg.tokens / limit) * 100);
          return (
            <div
              key={seg.key}
              className="h-full first:rounded-l-full last:rounded-r-full transition-all duration-200"
              style={{ width: `${share}%`, backgroundColor: seg.color }}
              title={`${seg.label}: ${formatTokenCount(seg.tokens)}`}
            />
          );
        })}
      </div>

      <div className="grid grid-cols-2 gap-1.5 text-[11px] text-[var(--soxu-text-tertiary)]">
        {segments.map((seg) => (
          <div
            key={seg.key}
            className="rounded-lg bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-hairline)] px-2.5 py-1.5 flex items-center justify-between gap-2"
          >
            <span className="inline-flex items-center gap-1.5 min-w-0">
              <span className="h-2 w-2 rounded-[2px] shrink-0" style={{ backgroundColor: seg.color }} />
              <span className="truncate">{seg.label}</span>
            </span>
            <span className="soxu-mono soxu-tabular text-[var(--soxu-text-secondary)] shrink-0">
              {formatTokenCount(seg.tokens)}
            </span>
          </div>
        ))}
      </div>

      {onCompact && (
        <Button
          size="sm"
          variant="secondary"
          className="w-full justify-center"
          onClick={() => {
            onCompact();
            onClose?.();
          }}
        >
          <Minimize2 className="h-3.5 w-3.5" strokeWidth={1.75} />
          <span>压缩冗余对话上下文</span>
        </Button>
      )}
    </div>
  );
}

/**
 * 输入框内嵌上下文窗口使用百分比圆环：
 * - 按已用比例使用既有主题语义色：绿（< 65%）、黄（65% - 88%）、红（>= 88%）；
 * - 窄屏支持切换为 Bottom Sheet 承载，杜绝溢出。
 */
export function ContextUsageRing({
  model,
  usedTokens,
  onCompact,
  onOpenChangeLocked,
  onOpenSheet,
  compact = false,
  className,
}: ContextUsageRingProps) {
  const [open, setOpen] = useState(false);
  const limit = MODEL_CONTEXT_LIMITS[model] ?? 200000;
  const rawPct = Math.min(100, Math.max(0.1, (usedTokens / limit) * 100));
  const pctLabel = rawPct >= 99.95 ? "100%" : `${rawPct.toFixed(1)}%`;

  const r = 7;
  const circumference = 2 * Math.PI * r;
  const offset = circumference * (1 - rawPct / 100);

  const ringColor =
    rawPct >= 88
      ? "var(--soxu-semantic-danger)"
      : rawPct >= 65
        ? "var(--soxu-semantic-warning)"
        : "var(--soxu-semantic-success)";

  const triggerBtn = (
    <button
      type="button"
      onClick={onOpenSheet}
      className={cn(
        "soxu-hit-44 inline-flex items-center gap-1.5 h-7 px-2 rounded-full border border-transparent hover:border-[var(--soxu-border-subtle)] hover:bg-[var(--soxu-interaction-hover)] transition-colors cursor-pointer text-[11.5px] soxu-tabular",
        className,
      )}
      aria-label={`上下文窗口用量 ${pctLabel}`}
    >
      <svg width="16" height="16" viewBox="0 0 18 18" className="-rotate-90 shrink-0" aria-hidden="true">
        <circle cx="9" cy="9" r={r} fill="none" stroke="var(--soxu-border-strong)" strokeWidth="2.2" />
        <circle
          cx="9"
          cy="9"
          r={r}
          fill="none"
          stroke={ringColor}
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          className="transition-all duration-200 ease-[var(--soxu-ease-out)]"
        />
      </svg>
      {!compact && (
        <span className="soxu-mono font-medium" style={{ color: ringColor }}>
          {pctLabel}
        </span>
      )}
    </button>
  );

  if (onOpenSheet) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{triggerBtn}</TooltipTrigger>
        <TooltipContent>上下文窗口已使用 {pctLabel}</TooltipContent>
      </Tooltip>
    );
  }

  return (
    <DropdownMenu
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        onOpenChangeLocked?.(next);
      }}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{triggerBtn}</DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>上下文窗口已使用 {pctLabel}</TooltipContent>
      </Tooltip>

      <DropdownMenuContent side="top" align="end" className="w-[min(276px,calc(100vw-24px))] p-3.5 rounded-2xl">
        <ContextUsagePanel model={model} usedTokens={usedTokens} onCompact={onCompact} onClose={() => setOpen(false)} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
