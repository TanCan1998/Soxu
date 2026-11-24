import { Minimize2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { MODEL_CONTEXT_LIMITS } from "@/lib/agent-capabilities";
import { cn } from "@/lib/utils";

interface ContextUsageRingProps {
  model: string;
  usedTokens: number;
  onCompact?: () => void;
  onOpenChangeLocked?: (open: boolean) => void;
  className?: string;
}

function formatTokenCount(n: number): string {
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}

/**
 * 输入框内嵌上下文窗口使用百分比小圆环：
 * 实时展示当前已用占比，点击弹出干净清晰的用量构成与一键压缩入口。
 */
export function ContextUsageRing({ model, usedTokens, onCompact, onOpenChangeLocked, className }: ContextUsageRingProps) {
  const [open, setOpen] = useState(false);
  const limit = MODEL_CONTEXT_LIMITS[model] ?? 200000;
  const pct = Math.min(100, Math.max(1, Math.round((usedTokens / limit) * 100)));

  const r = 7;
  const circumference = 2 * Math.PI * r;
  const offset = circumference * (1 - pct / 100);

  const ringColor =
    pct >= 85
      ? "var(--soxu-semantic-danger)"
      : pct >= 65
        ? "var(--soxu-semantic-warning)"
        : "var(--soxu-semantic-accent)";

  const sysTokens = Math.round(usedTokens * 0.14);
  const historyTokens = Math.round(usedTokens * 0.56);
  const codeTokens = Math.max(0, usedTokens - sysTokens - historyTokens);

  return (
    <DropdownMenu
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        onOpenChangeLocked?.(next);
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className={cn(
            "inline-flex items-center gap-1.5 h-7 px-2 rounded-full border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] hover:border-[var(--soxu-border-strong)] transition-colors cursor-pointer text-[11px] soxu-tabular text-[var(--soxu-text-secondary)]",
            className,
          )}
          aria-label={`上下文窗口已使用 ${pct}%`}
          title={`上下文窗口已使用 ${pct}%`}
        >
          <svg width="18" height="18" viewBox="0 0 18 18" className="-rotate-90 shrink-0" aria-hidden="true">
            <circle cx="9" cy="9" r={r} fill="none" stroke="var(--soxu-border-subtle)" strokeWidth="2.2" />
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
          <span className="soxu-mono text-[10.5px]">{pct}%</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64 p-3 flex flex-col gap-2.5">
        <div className="flex items-center justify-between text-xs">
          <span className="font-semibold text-[var(--soxu-text-primary)]">上下文窗口用量</span>
          <span className="soxu-mono soxu-tabular" style={{ color: ringColor }}>
            {pct}% ({formatTokenCount(usedTokens)} / {formatTokenCount(limit)})
          </span>
        </div>

        <div className="h-1.5 w-full rounded-full bg-[var(--soxu-surface-control)] overflow-hidden">
          <div
            className="h-full rounded-full transition-all duration-200"
            style={{ width: `${pct}%`, backgroundColor: ringColor }}
          />
        </div>

        <div className="grid grid-cols-3 gap-1.5 text-[11px] text-[var(--soxu-text-tertiary)]">
          <div className="rounded-md bg-[var(--soxu-surface-card-muted)] px-2 py-1.5">
            <div>系统规则</div>
            <div className="soxu-mono text-[var(--soxu-text-secondary)] mt-0.5">{formatTokenCount(sysTokens)}</div>
          </div>
          <div className="rounded-md bg-[var(--soxu-surface-card-muted)] px-2 py-1.5">
            <div>对话历史</div>
            <div className="soxu-mono text-[var(--soxu-text-secondary)] mt-0.5">{formatTokenCount(historyTokens)}</div>
          </div>
          <div className="rounded-md bg-[var(--soxu-surface-card-muted)] px-2 py-1.5">
            <div>代码引用</div>
            <div className="soxu-mono text-[var(--soxu-text-secondary)] mt-0.5">{formatTokenCount(codeTokens)}</div>
          </div>
        </div>

        {onCompact && (
          <Button
            size="sm"
            variant="secondary"
            className="w-full justify-center"
            onClick={() => {
              onCompact();
              setOpen(false);
            }}
          >
            <Minimize2 className="h-3.5 w-3.5" strokeWidth={1.75} />
            <span>压缩冗余对话上下文</span>
          </Button>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
