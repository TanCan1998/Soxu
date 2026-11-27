import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  ListOrdered,
  Pause,
  Pencil,
  Play,
  X,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n } from "@/lib/i18n";
import { formatShortcut } from "@/lib/useAdaptiveViewport";
import { cn } from "@/lib/utils";
import type { QueuedPromptItem } from "@/types";

interface MultiQueueBarProps {
  items: QueuedPromptItem[];
  paused: boolean;
  supportsLiveSteer: boolean;
  isMac?: boolean;
  docked?: boolean;
  onTogglePause: () => void;
  onPromoteSteer: (item: QueuedPromptItem) => void;
  onEditItem: (item: QueuedPromptItem) => void;
  onMoveItem: (id: string, dir: -1 | 1) => void;
  onRemoveItem: (id: string) => void;
}

/**
 * 多条可视化待发任务队列条（融合 Codex 贴合挂耳与 Oryn 队列动作）：
 * - docked=true 时作为内缩挂耳无缝咬合在主输入舱顶沿，消除刺眼全黄外框；
 * - 支持多条指令按序排队、随时暂停/恢复自动派发，增大按钮热区防止触屏误触。
 */
export function MultiQueueBar({
  items,
  paused,
  supportsLiveSteer,
  isMac = false,
  docked = false,
  onTogglePause,
  onPromoteSteer,
  onEditItem,
  onMoveItem,
  onRemoveItem,
}: MultiQueueBarProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(true);

  if (items.length === 0) return null;

  const head = items[0];

  return (
    <div
      className={cn(
        "overflow-hidden text-xs",
        docked
          ? "mx-3 sm:mx-4 rounded-t-[16px] rounded-b-none border border-b-0 border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] -mb-px"
          : "rounded-xl border border-[var(--soxu-semantic-warning-border)] bg-[var(--soxu-surface-card)]",
      )}
    >
      <div
        className={cn(
          "flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)]",
          docked ? "min-h-[38px] px-3.5 py-2" : "px-3 py-1.5",
        )}
      >
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex items-center gap-2 min-w-0 flex-1 text-left text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer bg-transparent border-0 p-0"
        >
          {items.length > 1 ? (
            expanded ? (
              <ChevronDown className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
            )
          ) : (
            <ListOrdered className="h-3.5 w-3.5 text-[var(--soxu-semantic-warning)] shrink-0" strokeWidth={1.75} />
          )}
          <span className="font-medium shrink-0 text-[var(--soxu-text-primary)]">{t("queue.title", { count: items.length })}</span>
          {paused && <Badge variant="warning">{t("queue.paused")}</Badge>}
          {(!expanded || items.length === 1) && (
            <span className="soxu-mono text-[11.5px] text-[var(--soxu-text-tertiary)] truncate">{head.text}</span>
          )}
        </button>

        <div className="flex items-center gap-1.5 shrink-0">
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={onTogglePause}
                aria-label={paused ? t("queue.resume") : t("queue.pause")}
                className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
              >
                {paused ? <Play className="h-3.5 w-3.5" strokeWidth={1.75} /> : <Pause className="h-3.5 w-3.5" strokeWidth={1.75} />}
              </button>
            </TooltipTrigger>
            <TooltipContent>{paused ? t("queue.resume") : t("queue.pause")}</TooltipContent>
          </Tooltip>

          {items.length === 1 && supportsLiveSteer && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => onPromoteSteer(head)}
                  aria-label={t("composer.promoteSteer")}
                  className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-semantic-running)] hover:bg-[var(--soxu-semantic-running-soft)] cursor-pointer"
                >
                  <Zap className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">
                {t("composer.promoteSteer")} ({formatShortcut("Mod+Enter", isMac)})
              </TooltipContent>
            </Tooltip>
          )}

          {items.length === 1 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => onEditItem(head)}
                  aria-label={t("queue.edit")}
                  className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-hover)] cursor-pointer"
                >
                  <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </TooltipTrigger>
              <TooltipContent>{t("queue.edit")}</TooltipContent>
            </Tooltip>
          )}

          {items.length === 1 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => onRemoveItem(head.id)}
                  aria-label={t("composer.cancelQueue")}
                  className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-semantic-danger)] hover:bg-[var(--soxu-semantic-danger-soft)] cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </TooltipTrigger>
              <TooltipContent>{t("composer.cancelQueue")}</TooltipContent>
            </Tooltip>
          )}
        </div>
      </div>

      {expanded && items.length > 1 && (
        <div className="divide-y divide-[var(--soxu-border-hairline)]">
          {items.map((item, idx) => (
            <div key={item.id} className="px-3.5 py-2 flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 min-w-0">
                <span className="soxu-tabular text-[10.5px] text-[var(--soxu-text-tertiary)] w-4 text-right shrink-0">
                  {idx + 1}.
                </span>
                <span className="soxu-mono truncate text-[var(--soxu-text-primary)]">{item.text}</span>
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  disabled={idx === 0}
                  onClick={() => onMoveItem(item.id, -1)}
                  aria-label={t("queue.moveUp")}
                  className={cn(
                    "soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer",
                    idx === 0 && "opacity-30 pointer-events-none",
                  )}
                >
                  <ArrowUp className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
                <button
                  type="button"
                  disabled={idx === items.length - 1}
                  onClick={() => onMoveItem(item.id, 1)}
                  aria-label={t("queue.moveDown")}
                  className={cn(
                    "soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer",
                    idx === items.length - 1 && "opacity-30 pointer-events-none",
                  )}
                >
                  <ArrowDown className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
                {supportsLiveSteer && (
                  <button
                    type="button"
                    onClick={() => onPromoteSteer(item)}
                    aria-label={t("composer.promoteSteer")}
                    className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-semantic-running)] hover:bg-[var(--soxu-semantic-running-soft)] cursor-pointer"
                  >
                    <Zap className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => onEditItem(item)}
                  aria-label={t("queue.edit")}
                  className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                >
                  <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
                <button
                  type="button"
                  onClick={() => onRemoveItem(item.id)}
                  aria-label={t("composer.cancelQueue")}
                  className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-semantic-danger)] hover:bg-[var(--soxu-semantic-danger-soft)] cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
