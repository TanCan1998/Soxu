import { ChevronRight, Clock } from "lucide-react";
import { useState } from "react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { SessionTelemetryStats } from "@/types";

interface SessionStatsPopoverProps {
  stats: SessionTelemetryStats;
  onOpenSheet?: () => void;
  compact?: boolean;
  className?: string;
}

function formatTotalTokens(n: number): string {
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

function formatDuration(sec: number, locale: string): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  if (locale.startsWith("en")) {
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }
  return m > 0 ? `${m}分${s}秒` : `${s}秒`;
}

export function SessionStatsPanel({ stats }: { stats: SessionTelemetryStats }) {
  const { locale, t } = useI18n();
  const [moreOpen, setMoreOpen] = useState(true);
  const speedText = `${stats.recentSpeed.toFixed(1)} tok/s`;

  const primaryRows = [
    { label: t("stats.turnsSteps"), value: `${stats.turns} / ${stats.steps}` },
    { label: t("stats.totalTokens"), value: formatTotalTokens(stats.totalTokens) },
    { label: t("stats.apiCost"), value: `$${stats.apiCostUsd.toFixed(2)}` },
    { label: t("stats.avgSpeed"), value: `${stats.avgSpeed.toFixed(1)} tok/s` },
    { label: t("stats.peakSpeed"), value: `${stats.peakSpeed.toFixed(1)} tok/s` },
    { label: t("stats.recentSpeed"), value: speedText },
    { label: t("stats.cacheHit"), value: `${stats.cacheHitPct.toFixed(1)}%` },
    { label: t("stats.cacheNote"), value: stats.cacheNote },
  ];

  const extraRows = [
    { label: t("stats.modelTime"), value: formatDuration(stats.modelDurationSec, locale) },
    { label: t("stats.toolTime"), value: formatDuration(stats.toolDurationSec, locale) },
    { label: t("stats.ttft"), value: formatDuration(stats.ttftSec, locale) },
  ];

  return (
    <div className="text-xs">
      <div className="text-[12px] font-medium text-[var(--soxu-text-secondary)] mb-2.5">{t("stats.title")}</div>

      <div className="flex flex-col gap-1.5 soxu-tabular">
        {primaryRows.map((row) => (
          <div key={row.label} className="flex items-center justify-between gap-3 leading-snug">
            <span className="text-[var(--soxu-text-tertiary)]">{row.label}</span>
            <span className="font-medium text-[var(--soxu-text-primary)] soxu-mono text-right">{row.value}</span>
          </div>
        ))}
      </div>

      <Collapsible open={moreOpen} onOpenChange={setMoreOpen} className="mt-2.5">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="inline-flex items-center gap-1 text-[11.5px] font-semibold text-[var(--soxu-text-primary)] hover:text-[var(--soxu-semantic-accent)] cursor-pointer transition-colors"
          >
            <span>{t("stats.more")}</span>
            <ChevronRight
              className={cn("h-3.5 w-3.5 transition-transform duration-150", moreOpen && "rotate-90")}
              strokeWidth={1.75}
            />
          </button>
        </CollapsibleTrigger>

        <CollapsibleContent className="soxu-collapsible overflow-hidden">
          <Separator className="my-2.5" />
          <div className="flex flex-col gap-1.5 soxu-tabular">
            {extraRows.map((row) => (
              <div key={row.label} className="flex items-center justify-between gap-3 leading-snug">
                <span className="text-[var(--soxu-text-tertiary)]">{row.label}</span>
                <span className="font-medium text-[var(--soxu-text-primary)] soxu-mono text-right">{row.value}</span>
              </div>
            ))}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

/**
 * 紧邻上下文百分比圆环右侧的「会话统计」胶囊与详单浮层：
 * - 胶囊实时显示绿色主题色图标与输出速度（tok/s），悬停展示简要标题；
 * - 窄屏下支持点击弹出无溢出的 Bottom Sheet。
 */
export function SessionStatsPopover({ stats, onOpenSheet, compact = false, className }: SessionStatsPopoverProps) {
  const { t } = useI18n();
  const speedText = `${stats.recentSpeed.toFixed(1)} tok/s`;

  const triggerBtn = (
    <button
      type="button"
      onClick={onOpenSheet}
      className={cn(
        "soxu-hit-44 inline-flex items-center gap-1.5 h-7 px-2.5 rounded-full border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-control)] hover:bg-[var(--soxu-interaction-selected)] transition-colors cursor-pointer text-[11.5px] soxu-tabular",
        compact && "px-2",
        className,
      )}
      aria-label={`${t("stats.title")} ${speedText}`}
    >
      <Clock className="h-3.5 w-3.5 text-[var(--soxu-semantic-success)] shrink-0" strokeWidth={1.75} />
      <span className="soxu-mono font-medium text-[var(--soxu-semantic-success)] whitespace-nowrap">
        {compact ? stats.recentSpeed.toFixed(0) : speedText}
      </span>
    </button>
  );

  if (onOpenSheet) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{triggerBtn}</TooltipTrigger>
        <TooltipContent>
          {t("stats.title")} {speedText}
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{triggerBtn}</DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>
          {t("stats.title")} {speedText}
        </TooltipContent>
      </Tooltip>

      <DropdownMenuContent
        side="top"
        align="end"
        className="w-[min(256px,calc(100vw-24px))] rounded-2xl border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-floating)] p-3.5"
      >
        <SessionStatsPanel stats={stats} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
