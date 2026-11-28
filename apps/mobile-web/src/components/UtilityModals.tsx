import { Clock, Cpu, QrCode, RotateCcw, Search, Sparkles } from "lucide-react";
import { useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { GitCommitDialog } from "@/components/GitCommitDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useI18n } from "@/lib/i18n";
import { formatShortcut, isImeComposing } from "@/lib/useAdaptiveViewport";
import type { ScheduleItem } from "@/types";

export type WorkbenchModalKind = null | "cmdk" | "pair" | "rewind" | "commit" | "schedules" | "history" | "onboarding";

interface UtilityModalsProps {
  modal: WorkbenchModalKind;
  onClose: () => void;
  onOpenModal: (m: WorkbenchModalKind) => void;
  onOpenSettings: () => void;
  onToggleTheme: () => void;
  branch?: string;
  isMac?: boolean;
  isCoarsePointer?: boolean;
  schedules: ScheduleItem[];
  onToggleSchedule: (id: string) => void;
  onFlashToast: (msg: string) => void;
}

export function UtilityModals({
  modal,
  onClose,
  onOpenModal,
  onOpenSettings,
  onToggleTheme,
  branch,
  isMac = false,
  isCoarsePointer = false,
  schedules,
  onToggleSchedule,
  onFlashToast,
}: UtilityModalsProps) {
  const { t } = useI18n();
  const [cmdkQuery, setCmdkQuery] = useState("");
  const [onboardingStep, setOnboardingStep] = useState(1);
  // 历史导入面板占位：真实原生历史扫描属 M2 后续交付，接通前展示空态。
  const nativeHistory: { id: string; cli: string; title: string; turns: number; updatedAt: string }[] = [];

  const cmdItems = [
    { sec: "工作区与会话", title: "feat/12-native-drivers · codex", sub: "2 项权限待确认", run: () => onClose() },
    { sec: "工作区与会话", title: "feat/8-ws-cursor-stream · pi", sub: "运行中 · 游标增量追平", run: () => onClose() },
    { sec: "对话正文", title: "crates/soxu-drivers/src/lib.rs · supports_live_steer", sub: "已完成热纠偏能力探测重构", run: () => onClose() },
    { sec: "系统动作", title: `打开独立系统设置面板 (${formatShortcut("Mod+,", isMac)})`, sub: "外观、字体、路由器与快捷键", run: () => { onClose(); onOpenSettings(); } },
    { sec: "系统动作", title: "打开手机扫码遥控配对", sub: "局域网 / Tailscale 6 位码连接", run: () => onOpenModal("pair") },
    { sec: "系统动作", title: "打开结构化 Git 提交编辑框", sub: "规范表情标题与隐私扫描", run: () => onOpenModal("commit") },
    { sec: "系统动作", title: "管理定时任务与周期巡检中心", sub: "Schedule & Heartbeat 调度", run: () => onOpenModal("schedules") },
    { sec: "系统动作", title: "切换纯黑深色与纯净浅色主题", sub: "Pure Black / Clean Light", run: () => { onToggleTheme(); onClose(); } },
  ].filter(
    (x) =>
      !cmdkQuery.trim() ||
      x.title.toLowerCase().includes(cmdkQuery.toLowerCase()) ||
      x.sub.toLowerCase().includes(cmdkQuery.toLowerCase()),
  );

  const cmdGroups = Array.from(
    cmdItems.reduce((map, it) => {
      const list = map.get(it.sec) ?? [];
      list.push(it);
      map.set(it.sec, list);
      return map;
    }, new Map<string, typeof cmdItems>()),
  );

  return (
    <>
      {/* 全局命令与检索面板（对齐 Oryn：偏顶停靠防键盘遮挡 + 16px 防 iOS 缩放 + 分组高亮） */}
      <Dialog
        open={modal === "cmdk"}
        onOpenChange={(o) => {
          if (!o) onClose();
        }}
      >
        <DialogContent
          showClose={false}
          className="top-[12dvh] translate-y-0 w-[min(var(--soxu-size-cmd-palette-width),94vw)] p-0 overflow-hidden bg-[var(--soxu-surface-floating)]"
        >
          <DialogHeader className="sr-only">
            <DialogTitle>全局命令与检索面板</DialogTitle>
            <DialogDescription>按关键词过滤工作区会话、文件与系统指令</DialogDescription>
          </DialogHeader>
          <div className="h-[52px] px-3.5 border-b border-[var(--soxu-border-subtle)] flex items-center gap-2.5">
            <Search className="h-4 w-4 text-[var(--soxu-text-tertiary)] shrink-0" strokeWidth={1.75} />
            <input
              autoFocus
              value={cmdkQuery}
              onChange={(e) => setCmdkQuery(e.target.value)}
              onKeyDown={(e) => {
                if (isImeComposing(e)) return;
                if (e.key === "Escape" && cmdkQuery.trim()) {
                  e.stopPropagation();
                  setCmdkQuery("");
                } else if (e.key === "Enter" && cmdItems[0]) {
                  e.preventDefault();
                  cmdItems[0].run();
                }
              }}
              placeholder="搜索工作区会话、代码文件、历史记录或系统指令..."
              className="flex-1 bg-transparent border-0 outline-none text-base sm:text-sm text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)]"
            />
            <button
              type="button"
              onClick={onClose}
              className="soxu-mono text-[10.5px] px-2 py-1 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
            >
              Esc
            </button>
          </div>
          <div className="max-h-[min(360px,56dvh)] overflow-y-auto p-2 flex flex-col gap-2 soxu-scroll">
            {cmdItems.length === 0 ? (
              <div className="py-10 text-center text-xs text-[var(--soxu-text-tertiary)]">
                未找到匹配的命令或会话条目
              </div>
            ) : (
              cmdGroups.map(([groupName, groupItems]) => (
                <div key={groupName} className="flex flex-col gap-0.5">
                  <div className="px-3 py-1 text-[11px] font-semibold text-[var(--soxu-text-tertiary)]">
                    {groupName}
                  </div>
                  {groupItems.map((it) => (
                    <button
                      key={it.title}
                      type="button"
                      onClick={it.run}
                      className="w-full min-h-[42px] rounded-lg px-3 py-2 text-left text-xs flex items-center justify-between gap-2 hover:bg-[var(--soxu-interaction-selected)] cursor-pointer transition-colors"
                    >
                      <div className="min-w-0">
                        <div className="font-medium text-[var(--soxu-text-primary)] truncate">{it.title}</div>
                        <div className="text-[11px] text-[var(--soxu-text-tertiary)] truncate">{it.sub}</div>
                      </div>
                      <Badge variant="neutral">{it.sec}</Badge>
                    </button>
                  ))}
                </div>
              ))
            )}
          </div>
          {!isCoarsePointer && (
            <div className="h-8 px-3.5 border-t border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-card)] flex items-center gap-4 text-[11px] text-[var(--soxu-text-tertiary)]">
              <span>Enter 执行首项</span>
              <span>Esc 清空或关闭</span>
            </div>
          )}
        </DialogContent>
      </Dialog>

      {/* 手机扫码配对 */}
      <Dialog open={modal === "pair"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="p-6 flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <QrCode className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>移动遥控扫码配对（一次性凭据）</span>
            </DialogTitle>
            <DialogDescription>手机浏览器打开后立即清除地址栏令牌片段，防止截屏泄露访问权限。</DialogDescription>
          </DialogHeader>
          <div className="flex flex-wrap items-center gap-4 py-1">
            <div className="bg-white p-3.5 rounded-xl text-black">
              <QrCode className="h-24 w-24" strokeWidth={1.5} />
            </div>
            <div className="flex-1 min-w-[200px] flex flex-col gap-1.5">
              <div className="text-xs text-[var(--soxu-text-secondary)]">6 位一次性数字配对码（5 分钟有效）：</div>
              <div className="text-2xl font-bold soxu-mono tracking-[0.18em]">849 217</div>
              <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] break-all">
                http://192.168.1.42:7698/#offer=eyJ2IjoyfQ
              </div>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* 检查点安全回退 */}
      <Dialog open={modal === "rewind"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="w-[min(480px,94vw)] p-6 flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <RotateCcw className="h-4 w-4 text-[var(--soxu-semantic-handoff)]" strokeWidth={1.75} />
              <span>检查点 #04 安全回退</span>
            </DialogTitle>
            <DialogDescription>所有代码回退均会自动创建隐藏暂存备份，随时可一键撤销还原。</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            {[
              { label: "1. 仅分叉当前对话（保留工作区代码不变 · 默认推荐）", danger: false },
              { label: "2. 仅回退工作树代码至检查点 7c4e91a", danger: false },
              { label: "3. 代码与对话上下文同步回退至 #04", danger: true },
            ].map((opt) => (
              <button
                key={opt.label}
                type="button"
                onClick={() => {
                  onClose();
                  onFlashToast(`已执行：${opt.label}`);
                }}
                className="w-full rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 text-left text-xs font-medium hover:border-[var(--soxu-border-strong)] hover:bg-[var(--soxu-interaction-selected)] cursor-pointer transition-colors"
                style={opt.danger ? { color: "var(--soxu-semantic-danger)", borderColor: "var(--soxu-semantic-danger-border)" } : undefined}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      {/* 结构化拆分式 Git 提交弹窗 */}
      <GitCommitDialog
        open={modal === "commit"}
        onOpenChange={(o) => !o && onClose()}
        branch={branch}
        onFlashToast={onFlashToast}
      />

      {/* 定时任务与周期巡检 */}
      <Dialog open={modal === "schedules"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="w-[min(640px,94vw)] p-6 flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Clock className="h-4 w-4 text-[var(--soxu-semantic-running)]" strokeWidth={1.75} />
              <span>定时任务与周期巡检调度中心</span>
            </DialogTitle>
            <DialogDescription>按定时计划自动新建隔离工作树代理或周期唤醒活跃会话巡检</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2.5 max-h-[55vh] overflow-y-auto soxu-scroll">
            {schedules.map((s) => (
              <div
                key={s.id}
                className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3 flex items-center justify-between gap-3"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold truncate">{s.name}</span>
                    <Badge variant={s.kind === "Heartbeat" ? "handoff" : "running"}>
                      {s.kind === "Heartbeat" ? "周期巡检" : "定时任务"}
                    </Badge>
                    {s.unreadFail && <Badge variant="danger">未读失败</Badge>}
                  </div>
                  <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] mt-1">
                    {s.cron} · {s.human} · {s.target}
                  </div>
                </div>
                <Button size="sm" variant={s.active ? "secondary" : "default"} onClick={() => onToggleSchedule(s.id)}>
                  {s.active ? "暂停" : "恢复"}
                </Button>
              </div>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      {/* 本地原生历史会话导入 */}
      <Dialog open={modal === "history"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="p-6 flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>导入本机原生代理历史会话</span>
            </DialogTitle>
            <DialogDescription>支持原位续接原始编号或分叉注入全新隔离工作树上下文。</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            {nativeHistory.length === 0 ? (
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-6 text-center text-xs text-[var(--soxu-text-tertiary)]">
                原生会话历史扫描尚未接入：待守护进程历史导入能力交付后在此展示。
              </div>
            ) : (
              nativeHistory.map((h) => (
                <button
                  key={h.id}
                  type="button"
                  onClick={() => {
                    onClose();
                    onFlashToast(`已加载 ${h.cli} 历史会话：${h.title}`);
                  }}
                  className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3 text-left hover:border-[var(--soxu-border-strong)] hover:bg-[var(--soxu-interaction-selected)] cursor-pointer transition-colors"
                >
                  <div className="flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)]">
                    <span className="inline-flex items-center gap-1.5 soxu-mono font-semibold text-[var(--soxu-text-primary)]">
                      <AgentIcon agent={h.cli} className="h-3.5 w-3.5 shrink-0" />
                      <span>{h.cli}</span>
                    </span>
                    <span>
                      {h.turns} 轮 · {h.updatedAt}
                    </span>
                  </div>
                  <div className="text-xs font-medium mt-1">{h.title}</div>
                </button>
              ))
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* 首次三步引导（仅首次或在设置中手动重开，不常驻） */}
      <Dialog open={modal === "onboarding"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="p-6 flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>
                第 {onboardingStep} / 3 步 ·{" "}
                {onboardingStep === 1
                  ? "本机路由与原生驱动只读自检"
                  : onboardingStep === 2
                    ? "绑定首个项目目录与共享编译缓存"
                    : "无项目对话、手机遥控与快捷键速览"}
              </span>
            </DialogTitle>
            <DialogDescription>
              {onboardingStep === 1
                ? "已自动探测 6 个可用原生命令行驱动与本地模型路由器，全部全局目录保持只读无侵入。"
                : onboardingStep === 2
                  ? "已启用跨工作树共享构建产物缓存目录，新增隔离工作树秒级启动。"
                  : "支持直接开启无项目快速对话或按项目工作树并行编排；按 Ctrl+, 随时打开七分区设置面板。"}
            </DialogDescription>
          </DialogHeader>
          <div className="flex items-center justify-between pt-2">
            <Button
              variant="secondary"
              disabled={onboardingStep === 1}
              onClick={() => setOnboardingStep((s) => Math.max(1, s - 1))}
            >
              上一步
            </Button>
            <Button
              variant="default"
              onClick={() => {
                if (onboardingStep < 3) setOnboardingStep((s) => s + 1);
                else {
                  setOnboardingStep(1);
                  onClose();
                  onFlashToast(t("onboarding.welcomeToast"));
                }
              }}
            >
              {onboardingStep === 3 ? t("onboarding.cta") : "下一步"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
