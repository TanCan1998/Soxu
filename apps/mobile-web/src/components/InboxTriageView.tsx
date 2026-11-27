import {
  AlertTriangle,
  ArrowUpRight,
  Check,
  CheckCheck,
  Clock,
  FolderGit2,
  Inbox,
  Shield,
} from "lucide-react";
import { useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { ApprovalItem, AskQuestionState, WorkspaceBucket, WorkspaceItem } from "@/types";

interface InboxTriageViewProps {
  workspaces: WorkspaceItem[];
  approvals: ApprovalItem[];
  askQuestion?: AskQuestionState;
  onResolveApproval: (id: string, resolution: "approved" | "session" | "rejected", note?: string) => void;
  onAnswerAsk?: (optionId: string) => void;
  onOpenWorkspace: (workspaceId: string) => void;
  onMarkAllAttentionRead?: () => void;
}

const SECTION_ORDER: WorkspaceBucket[] = ["needs_input", "failed", "attention", "running"];

function bucketTitle(bucket: WorkspaceBucket): string {
  switch (bucket) {
    case "needs_input":
      return "等你处理";
    case "failed":
      return "出错";
    case "attention":
      return "已完成未读";
    case "running":
      return "运行中";
    case "done":
      return "空闲";
  }
}

function bucketDot(bucket: WorkspaceBucket): string {
  switch (bucket) {
    case "needs_input":
      return "bg-[var(--soxu-semantic-warning)]";
    case "failed":
      return "bg-[var(--soxu-semantic-danger)]";
    case "attention":
      return "bg-[var(--soxu-semantic-accent)]";
    case "running":
      return "bg-[var(--soxu-semantic-running)] soxu-live-dot";
    case "done":
      return "bg-[var(--soxu-text-tertiary)]";
  }
}

/**
 * 跨工作区收件箱分诊台（对标 Lynricsy/Oryn Inbox Triage 实图）：
 * - 顶部大号待处理总数 + 状态筛选胶囊 + [按状态 | 按项目] 分组切换；
 * - 按 等你处理 -> 出错 -> 已完成未读 -> 运行中 严格排序；
 * - 待审批命令与提问选择支持在行内一触作答（高危命令强制二次解锁，不削弱安全边界）。
 */
export function InboxTriageView({
  workspaces,
  approvals,
  askQuestion,
  onResolveApproval,
  onAnswerAsk,
  onOpenWorkspace,
  onMarkAllAttentionRead,
}: InboxTriageViewProps) {
  const { t } = useI18n();
  const [groupBy, setGroupBy] = useState<"status" | "project">("status");
  const [filter, setFilter] = useState<"all" | WorkspaceBucket>("all");
  const [unlockedHighRisk, setUnlockedHighRisk] = useState<Record<string, boolean>>({});

  const activeWorkspaces = workspaces.filter((w) => !w.archived && w.bucket !== "done");
  const pendingApprovals = approvals.filter((a) => !a.resolution);

  const counts = {
    all: activeWorkspaces.length,
    needs_input: activeWorkspaces.filter((w) => w.bucket === "needs_input").length,
    failed: activeWorkspaces.filter((w) => w.bucket === "failed").length,
    attention: activeWorkspaces.filter((w) => w.bucket === "attention").length,
    running: activeWorkspaces.filter((w) => w.bucket === "running").length,
  };

  const actionableTotal = counts.needs_input + counts.failed + counts.attention;
  const visibleItems =
    filter === "all" ? activeWorkspaces : activeWorkspaces.filter((w) => w.bucket === filter);

  function renderWorkspaceCard(w: WorkspaceItem) {
    const matchedApprovals = pendingApprovals.filter((a) => a.workspaceId === w.id);
    const isNeedsInput = w.bucket === "needs_input";

    return (
      <article
        key={w.id}
        className={cn(
          "rounded-2xl p-3.5 sm:p-4 flex flex-col gap-2.5 border transition-colors",
          isNeedsInput
            ? "bg-[var(--soxu-semantic-accent-soft)]/50 border-[var(--soxu-semantic-accent-border)]"
            : w.bucket === "failed"
              ? "bg-[var(--soxu-surface-card)] border-[var(--soxu-semantic-danger-border)]"
              : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)]",
        )}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex flex-col gap-0.5">
            <div className="flex items-center gap-2 min-w-0">
              <span className={cn("h-2 w-2 rounded-full shrink-0", bucketDot(w.bucket))} />
              <span className="soxu-mono text-xs sm:text-[13px] font-semibold text-[var(--soxu-text-primary)] truncate">
                {w.branch}
              </span>
              {w.label && <Badge variant="neutral">{w.label}</Badge>}
            </div>
            <div className="flex items-center gap-1.5 text-[11px] text-[var(--soxu-text-tertiary)] pl-4">
              <AgentIcon agent={w.agent} className="h-3.5 w-3.5 shrink-0" />
              <span className="soxu-mono">{w.project}</span>
              <span>·</span>
              <span className="soxu-mono">{w.agent}</span>
              <span>·</span>
              <span className="soxu-tabular">{w.diffStat}</span>
            </div>
          </div>

          <Button
            size="sm"
            variant={isNeedsInput ? "default" : "ghost"}
            onClick={() => onOpenWorkspace(w.id)}
            className="shrink-0"
          >
            <span>{t("inbox.openSession")}</span>
            <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} />
          </Button>
        </div>

        <div className="text-xs text-[var(--soxu-text-secondary)] pl-4 leading-relaxed">
          {w.activeIntent ?? w.summary}
        </div>

        {/* 行内一触作答（提问 + 审批） */}
        {isNeedsInput && askQuestion && !askQuestion.submitted && onAnswerAsk && (
          <div className="ml-0 sm:ml-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-sidebar)] p-3 flex flex-col gap-2">
            <div className="text-xs font-medium text-[var(--soxu-text-primary)]">{askQuestion.prompt}</div>
            <div className="flex flex-wrap gap-1.5">
              {askQuestion.options.map((opt) => (
                <Button
                  key={opt.id}
                  size="sm"
                  variant={opt.recommended ? "default" : "secondary"}
                  onClick={() => onAnswerAsk(opt.id)}
                >
                  {opt.label}
                </Button>
              ))}
            </div>
          </div>
        )}

        {matchedApprovals.map((apr) => {
          const unlocked = Boolean(unlockedHighRisk[apr.id]);
          return (
            <div
              key={apr.id}
              className={cn(
                "ml-0 sm:ml-4 rounded-xl border bg-[var(--soxu-surface-sidebar)] p-3 flex flex-col gap-2",
                apr.highRisk ? "border-[var(--soxu-semantic-danger-border)]" : "border-[var(--soxu-border-subtle)]",
              )}
            >
              <div className="flex items-center justify-between gap-2 text-xs">
                <span className="inline-flex items-center gap-1.5 font-medium text-[var(--soxu-text-primary)]">
                  {apr.highRisk ? (
                    <AlertTriangle className="h-3.5 w-3.5 text-[var(--soxu-semantic-danger)] shrink-0" strokeWidth={1.75} />
                  ) : (
                    <Shield className="h-3.5 w-3.5 text-[var(--soxu-semantic-warning)] shrink-0" strokeWidth={1.75} />
                  )}
                  <span>{apr.toolKind}</span>
                </span>
                <Badge variant={apr.highRisk ? "danger" : "warning"}>
                  {apr.highRisk ? "高危拦截" : "等待审批"}
                </Badge>
              </div>
              <div className="rounded-lg border border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-terminal)] px-2.5 py-1.5 text-[11.5px] soxu-mono text-[var(--soxu-text-code)] overflow-x-auto">
                $ {apr.command}
              </div>
              <div className="text-[11px] text-[var(--soxu-text-tertiary)]">{apr.riskNote}</div>
              {apr.highRisk ? (
                <div className="flex flex-wrap items-center gap-2 pt-0.5">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => onResolveApproval(apr.id, "rejected", "请在隔离临时目录安全执行")}
                  >
                    拒绝并指导
                  </Button>
                  {!unlocked ? (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => setUnlockedHighRisk((p) => ({ ...p, [apr.id]: true }))}
                    >
                      解锁二次确认
                    </Button>
                  ) : (
                    <Button size="sm" variant="destructive" onClick={() => onResolveApproval(apr.id, "approved")}>
                      <Check className="h-3.5 w-3.5" strokeWidth={2} />
                      <span>确认单次放行</span>
                    </Button>
                  )}
                </div>
              ) : (
                <div className="flex flex-wrap items-center gap-2 pt-0.5">
                  <Button size="sm" variant="default" onClick={() => onResolveApproval(apr.id, "approved")}>
                    允许一次
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => onResolveApproval(apr.id, "session")}>
                    本会话允许
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => onResolveApproval(apr.id, "rejected", "请补充验证后重新申请")}
                  >
                    拒绝
                  </Button>
                </div>
              )}
            </div>
          );
        })}
      </article>
    );
  }

  return (
    <section className="flex flex-col gap-4 w-full">
      {/* 顶部大标题 + 分组开关 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-baseline gap-2.5">
          <span className="inline-flex items-center gap-1.5 text-sm font-semibold text-[var(--soxu-text-primary)]">
            <Inbox className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
            <span>{t("inbox.title")}</span>
          </span>
          <span className="text-2xl font-bold soxu-tabular text-[var(--soxu-semantic-accent)]">{actionableTotal}</span>
          <span className="text-xs text-[var(--soxu-text-secondary)]">{t("inbox.pendingCount")}</span>
        </div>

        <div className="inline-flex rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]">
          {(["status", "project"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              onClick={() => setGroupBy(mode)}
              className={cn(
                "px-2.5 py-1 rounded-md text-xs cursor-pointer transition-colors",
                groupBy === mode
                  ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-medium"
                  : "text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-secondary)]",
              )}
            >
              {mode === "status" ? t("inbox.groupByStatus") : t("inbox.groupByProject")}
            </button>
          ))}
        </div>
      </div>

      {/* 状态快筛胶囊 */}
      <div className="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar pb-0.5">
        {(
          [
            { id: "all", label: `${t("inbox.filterAll")} ${counts.all}` },
            { id: "needs_input", label: `${t("inbox.filterNeedsInput")} ${counts.needs_input}` },
            { id: "failed", label: `${t("inbox.filterFailed")} ${counts.failed}` },
            { id: "attention", label: `${t("inbox.filterAttention")} ${counts.attention}` },
            { id: "running", label: `${t("inbox.filterRunning")} ${counts.running}` },
          ] as const
        ).map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setFilter(item.id)}
            className={cn(
              "min-h-[32px] px-3 py-1 rounded-full text-xs font-medium whitespace-nowrap border cursor-pointer transition-colors soxu-tabular",
              filter === item.id
                ? "bg-[var(--soxu-action-inverse)] text-[var(--soxu-action-on-inverse)] border-transparent font-semibold"
                : "bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)] border-[var(--soxu-border-subtle)] hover:border-[var(--soxu-border-strong)]",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>

      {visibleItems.length === 0 ? (
        <div className="rounded-2xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] p-8 text-center text-xs text-[var(--soxu-text-tertiary)]">
          {t("inbox.empty")}
        </div>
      ) : groupBy === "status" ? (
        <div className="flex flex-col gap-4">
          {SECTION_ORDER.map((bucket) => {
            const list = visibleItems.filter((w) => w.bucket === bucket);
            if (list.length === 0) return null;
            return (
              <div key={bucket} className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2 px-1">
                  <span className="inline-flex items-center gap-2 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                    <span className={cn("h-2 w-2 rounded-full", bucketDot(bucket))} />
                    <span>{bucketTitle(bucket)}</span>
                    <span className="soxu-tabular text-[var(--soxu-text-tertiary)]">({list.length})</span>
                  </span>
                  {bucket === "attention" && onMarkAllAttentionRead && (
                    <button
                      type="button"
                      onClick={onMarkAllAttentionRead}
                      className="inline-flex items-center gap-1 text-[11px] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                    >
                      <CheckCheck className="h-3.5 w-3.5" strokeWidth={1.75} />
                      <span>{t("inbox.markAllRead")}</span>
                    </button>
                  )}
                </div>
                <div className="flex flex-col gap-2.5">{list.map(renderWorkspaceCard)}</div>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="flex flex-col gap-4">
          {Array.from(
            visibleItems.reduce((map, w) => {
              const arr = map.get(w.project) ?? [];
              arr.push(w);
              map.set(w.project, arr);
              return map;
            }, new Map<string, WorkspaceItem[]>()),
          ).map(([proj, items]) => (
            <div key={proj} className="flex flex-col gap-2">
              <div className="flex items-center gap-2 px-1 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                <FolderGit2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{proj}</span>
                <span className="soxu-tabular text-[var(--soxu-text-tertiary)]">({items.length})</span>
              </div>
              <div className="flex flex-col gap-2.5">
                {items
                  .slice()
                  .sort((a, b) => SECTION_ORDER.indexOf(a.bucket) - SECTION_ORDER.indexOf(b.bucket))
                  .map(renderWorkspaceCard)}
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="flex items-center gap-1.5 text-[11px] text-[var(--soxu-text-tertiary)] px-1">
        <Clock className="h-3 w-3 shrink-0" strokeWidth={1.75} />
        <span>队列顺序严格按照：等你处理 &gt; 出错 &gt; 已完成未读 &gt; 运行中 实时聚簇</span>
      </div>
    </section>
  );
}
