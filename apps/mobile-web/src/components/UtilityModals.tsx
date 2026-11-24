import { Check, Clock, Cpu, GitCommit, QrCode, RotateCcw, Search, Sparkles } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { NATIVE_HISTORY } from "@/mock-data";
import type { ScheduleItem } from "@/types";

export type WorkbenchModalKind = null | "cmdk" | "pair" | "rewind" | "commit" | "schedules" | "history" | "onboarding";

interface UtilityModalsProps {
  modal: WorkbenchModalKind;
  onClose: () => void;
  onOpenModal: (m: WorkbenchModalKind) => void;
  onOpenSettings: () => void;
  onToggleTheme: () => void;
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
  schedules,
  onToggleSchedule,
  onFlashToast,
}: UtilityModalsProps) {
  const [cmdkQuery, setCmdkQuery] = useState("");
  const [onboardingStep, setOnboardingStep] = useState(1);

  const cmdItems = [
    { sec: "会话", title: "feat/12-native-drivers · codex", run: () => onClose() },
    { sec: "文件", title: "crates/soxu-drivers/src/lib.rs", run: () => onClose() },
    { sec: "设置", title: "打开独立设置界面 (Ctrl+,)", run: () => { onClose(); onOpenSettings(); } },
    { sec: "工具", title: "打开手机扫码配对 (#offer=)", run: () => onOpenModal("pair") },
    { sec: "工具", title: "打开 Git Emoji AI 提交预览", run: () => onOpenModal("commit") },
    { sec: "调度", title: "管理 Schedules & Heartbeats 定时任务", run: () => onOpenModal("schedules") },
    { sec: "外观", title: "切换 Pure Black / Clean Light 主题", run: () => { onToggleTheme(); onClose(); } },
  ].filter((x) => !cmdkQuery.trim() || x.title.toLowerCase().includes(cmdkQuery.toLowerCase()));

  return (
    <>
      {/* Cmd+K 全局命令面板 */}
      <Dialog
        open={modal === "cmdk"}
        onOpenChange={(o) => {
          if (!o) onClose();
        }}
      >
        <DialogContent
          showClose={false}
          className="w-[min(var(--soxu-size-cmd-palette-width),94vw)] p-0 overflow-hidden bg-[var(--soxu-surface-floating)]"
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
                if (e.key === "Escape" && cmdkQuery.trim()) {
                  e.stopPropagation();
                  setCmdkQuery("");
                } else if (e.key === "Enter" && cmdItems[0]) {
                  e.preventDefault();
                  cmdItems[0].run();
                }
              }}
              placeholder="搜索工作区会话、代码文件、历史记录或命令..."
              className="flex-1 bg-transparent border-0 outline-none text-sm text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)]"
            />
            <kbd className="soxu-mono text-[10.5px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-tertiary)]">
              Esc
            </kbd>
          </div>
          <div className="max-h-[340px] overflow-y-auto p-2 flex flex-col gap-0.5 soxu-scroll">
            {cmdItems.length === 0 ? (
              <div className="py-10 text-center text-xs text-[var(--soxu-text-tertiary)]">
                未找到与 &ldquo;{cmdkQuery}&rdquo; 匹配的命令或会话
              </div>
            ) : (
              cmdItems.map((it) => (
                <button
                  key={it.title}
                  type="button"
                  onClick={it.run}
                  className="w-full rounded-lg px-3 py-2 text-left text-xs flex items-center justify-between gap-2 hover:bg-[var(--soxu-interaction-selected)] cursor-pointer transition-colors"
                >
                  <span>{it.title}</span>
                  <Badge variant="neutral">{it.sec}</Badge>
                </button>
              ))
            )}
          </div>
          <div className="h-8 px-3.5 border-t border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-card)] flex items-center gap-4 text-[11px] text-[var(--soxu-text-tertiary)]">
            <span>Enter 执行首项</span>
            <span>Esc 清空或关闭</span>
          </div>
        </DialogContent>
      </Dialog>

      {/* 手机扫码配对 */}
      <Dialog open={modal === "pair"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <QrCode className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>手机遥控扫码配对（一次性 #offer= 令牌）</span>
            </DialogTitle>
            <DialogDescription>手机浏览器打开后立刻清除地址栏片段，防止截屏泄露访问凭证。</DialogDescription>
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

      {/* Checkpoint 安全回退 */}
      <Dialog open={modal === "rewind"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="w-[min(480px,94vw)]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <RotateCcw className="h-4 w-4 text-[var(--soxu-semantic-handoff)]" strokeWidth={1.75} />
              <span>检查点 #04 安全回退</span>
            </DialogTitle>
            <DialogDescription>所有代码回退动作均会先自动创建 Safety Stash 备份点，随时可恢复。</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            {[
              { label: "1. 仅分叉对话（保留当前磁盘代码不变 · 默认安全）", danger: false },
              { label: "2. 仅回退工作树代码至 commit 7c4e91a", danger: false },
              { label: "3. 代码与对话同步完整回退至 #04", danger: true },
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

      {/* Git Emoji AI 提交预览 */}
      <Dialog open={modal === "commit"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <GitCommit className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>Git Emoji AI 提交与隐私门禁预检</span>
            </DialogTitle>
            <DialogDescription>使用轻量模型按仓库规范生成提交信息，并在提交前预检个人路径与密钥。</DialogDescription>
          </DialogHeader>
          <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-3 text-xs soxu-code-block">
            {`${"\u2728"} feat(drivers): 补齐原生 CLI 热纠偏能力探测与 TempDir 单测`}
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> Git Emoji 合规
            </Badge>
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> 无个人路径泄露
            </Badge>
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> Gitleaks 通过
            </Badge>
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={onClose}>
              取消
            </Button>
            <Button
              variant="default"
              onClick={() => {
                onClose();
                onFlashToast("已完成 Git Emoji 提交预演");
              }}
            >
              确认提交
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Schedules & Heartbeats */}
      <Dialog open={modal === "schedules"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="w-[min(640px,94vw)]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Clock className="h-4 w-4 text-[var(--soxu-semantic-running)]" strokeWidth={1.75} />
              <span>Schedules &amp; Heartbeats 自动化调度中心</span>
            </DialogTitle>
            <DialogDescription>Cron 定时新建隔离 Worktree 代理或周期唤醒活跃会话巡检</DialogDescription>
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
                    <Badge variant={s.kind === "Heartbeat" ? "handoff" : "running"}>{s.kind}</Badge>
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

      {/* 本地 CLI 历史导入 */}
      <Dialog open={modal === "history"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>导入本机 CLI 原生历史会话</span>
            </DialogTitle>
            <DialogDescription>支持原位续接 Native ID 或分叉注入全新 Worktree 上下文。</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            {NATIVE_HISTORY.map((h) => (
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
                  <span className="soxu-mono font-semibold text-[var(--soxu-text-primary)]">{h.cli}</span>
                  <span>
                    {h.turns} 轮 · {h.updatedAt}
                  </span>
                </div>
                <div className="text-xs font-medium mt-1">{h.title}</div>
              </button>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      {/* 首次 3 步引导 */}
      <Dialog open={modal === "onboarding"} onOpenChange={(o) => !o && onClose()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>
                Step {onboardingStep}/3 ·{" "}
                {onboardingStep === 1
                  ? "本机 Magpie 与原生 CLI 只读自检"
                  : onboardingStep === 2
                    ? "绑定首个 Git 项目与共享构建缓存"
                    : "极简布局、手机遥控与快捷键速览"}
              </span>
            </DialogTitle>
            <DialogDescription>
              {onboardingStep === 1
                ? "已自动探测 6 个可用原生 CLI 与本地 Magpie 网关，所有全局配置目录保持只读无侵入。"
                : onboardingStep === 2
                  ? "已启用外置共享 target 缓存目录（CARGO_TARGET_DIR），新增 Worktree 秒级启动。"
                  : "非高频工具已默认折叠在顶部悬停收纳区；按 Ctrl+, 随时打开 7 分区独立设置。"}
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
                  onFlashToast("欢迎开始使用 Soxu 工作台");
                }
              }}
            >
              {onboardingStep === 3 ? "开始使用 Soxu" : "下一步"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
