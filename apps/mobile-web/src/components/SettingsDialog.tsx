import {
  ChevronDown,
  Cpu,
  GitBranch,
  Keyboard,
  Lock,
  Monitor,
  Moon,
  Palette,
  QrCode,
  RefreshCw,
  ShieldAlert,
  Smartphone,
  Sparkles,
  Sun,
  Type,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import {
  CODE_FONT_PRESETS,
  DEFAULT_PREFERENCES,
  UI_FONT_PRESETS,
  type CodeFontPresetId,
  type SoxuPreferences,
  type ThemeMode,
  type UiFontPresetId,
} from "@/lib/preferences";
import { cn } from "@/lib/utils";
import { CLI_OPTIONS } from "@/mock-data";

export type SettingsSectionId = "appearance" | "fonts" | "magpie" | "git" | "security" | "mobile" | "about";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialSection?: SettingsSectionId;
  prefs: SoxuPreferences;
  onUpdatePrefs: (patch: Partial<SoxuPreferences>, themeButtonEl?: HTMLElement | null) => void;
  magpieMode: "magpie_managed" | "standalone_native";
  onToggleMagpie: () => void;
  onOpenOnboarding?: () => void;
  onFlashToast: (msg: string) => void;
}

const SECTIONS: { id: SettingsSectionId; label: string; icon: typeof Palette }[] = [
  { id: "appearance", label: "外观与主题", icon: Palette },
  { id: "fonts", label: "字体与排版", icon: Type },
  { id: "magpie", label: "Agent 与 Magpie", icon: Cpu },
  { id: "git", label: "工作区与 Git", icon: GitBranch },
  { id: "security", label: "安全与审批红线", icon: ShieldAlert },
  { id: "mobile", label: "移动遥控与配对", icon: Smartphone },
  { id: "about", label: "快捷键与关于", icon: Keyboard },
];

export function SettingsDialog({
  open,
  onOpenChange,
  initialSection = "appearance",
  prefs,
  onUpdatePrefs,
  magpieMode,
  onToggleMagpie,
  onOpenOnboarding,
  onFlashToast,
}: SettingsDialogProps) {
  const [section, setSection] = useState<SettingsSectionId>(initialSection);
  const [tokensOpen, setTokensOpen] = useState(false);
  const [pairCode, setPairCode] = useState("849 217");

  useEffect(() => {
    if (open) setSection(initialSection);
  }, [open, initialSection]);

  const activeUiPreset = UI_FONT_PRESETS.find((x) => x.id === prefs.uiFontPreset) ?? UI_FONT_PRESETS[0];
  const activeCodePreset = CODE_FONT_PRESETS.find((x) => x.id === prefs.codeFontPreset) ?? CODE_FONT_PRESETS[0];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(840px,95vw)] h-[min(600px,88dvh)] p-0 overflow-hidden flex flex-col md:flex-row">
        <DialogHeader className="sr-only">
          <DialogTitle>Soxu 设置</DialogTitle>
          <DialogDescription>配置纯黑外观、开源双字体独立或跟随设置、Magpie 路由及安全审批</DialogDescription>
        </DialogHeader>

        {/* 左侧导航轨（桌面端） / 顶部胶囊条（移动端） */}
        <nav
          aria-label="设置分区"
          className="md:w-[210px] shrink-0 bg-[var(--soxu-surface-sidebar)] border-b md:border-b-0 md:border-r border-[var(--soxu-border-hairline)] p-3 flex md:flex-col gap-1 overflow-x-auto md:overflow-y-auto soxu-no-scrollbar"
        >
          <div className="hidden md:flex items-center justify-between px-2 pb-2 mb-1 border-b border-[var(--soxu-border-hairline)]">
            <span className="text-xs font-semibold tracking-wide">系统设置</span>
            <kbd className="text-[10px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-tertiary)]">
              Ctrl+,
            </kbd>
          </div>
          {SECTIONS.map((s) => {
            const Icon = s.icon;
            const active = section === s.id;
            return (
              <button
                key={s.id}
                type="button"
                onClick={() => setSection(s.id)}
                className={cn(
                  "flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-xs font-medium whitespace-nowrap transition-colors duration-150 cursor-pointer shrink-0",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)]"
                    : "text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                )}
              >
                <Icon className="h-4 w-4 shrink-0" strokeWidth={1.75} />
                <span>{s.label}</span>
              </button>
            );
          })}
        </nav>

        {/* 右侧设置内容区 */}
        <div className="flex-1 overflow-y-auto p-5 flex flex-col gap-5 soxu-scroll">
          {section === "appearance" && (
            <div className="flex flex-col gap-5 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">外观与主题</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                  深色主题采用零色偏 Pure Black 中性灰阶层级；主题切换触发 340ms 径向扩圆过渡。
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
                {(
                  [
                    { id: "dark", title: "Pure Black", desc: "纯黑 #000000 画布", icon: Moon, bg: "#000000", fg: "#ececec" },
                    { id: "light", title: "Clean Light", desc: "日光中性 #FFFFFF", icon: Sun, bg: "#ffffff", fg: "#0d0d0d" },
                    { id: "system", title: "跟随系统", desc: "同步操作系统明暗", icon: Monitor, bg: "#171717", fg: "#ececec" },
                  ] as const
                ).map((item) => {
                  const Icon = item.icon;
                  const active = prefs.themeMode === item.id;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={(e) => onUpdatePrefs({ themeMode: item.id as ThemeMode }, e.currentTarget)}
                      className={cn(
                        "rounded-xl border p-3 text-left transition-all duration-150 cursor-pointer flex flex-col gap-2",
                        active
                          ? "border-[var(--soxu-border-focus-ring)] bg-[var(--soxu-surface-raised)]"
                          : "border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] hover:border-[var(--soxu-border-strong)]",
                      )}
                    >
                      <div
                        className="h-10 rounded-lg border border-[var(--soxu-border-subtle)] flex items-center justify-between px-2.5"
                        style={{ backgroundColor: item.bg, color: item.fg }}
                      >
                        <Icon className="h-4 w-4" strokeWidth={1.75} />
                        <span className="text-[10px] soxu-mono">Aa Bb</span>
                      </div>
                      <div>
                        <div className="text-xs font-semibold">{item.title}</div>
                        <div className="text-[11px] text-[var(--soxu-text-tertiary)]">{item.desc}</div>
                      </div>
                    </button>
                  );
                })}
              </div>

              <Separator />

              <div className="flex items-center justify-between gap-4">
                <div>
                  <div className="text-xs font-medium">紧凑高密度工作区行距</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    工作区树与标签页启用 32px 高密度模式（关闭为 38px 舒适模式）
                  </div>
                </div>
                <Switch
                  checked={prefs.density === "compact"}
                  onCheckedChange={(v) => onUpdatePrefs({ density: v ? "compact" : "comfortable" })}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div>
                  <div className="text-xs font-medium">减弱动效（Reduce Motion）</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    跳过主题扩圆与手风琴过渡，瞬时响应一切状态切换（同步 GPUI delta=1）
                  </div>
                </div>
                <Switch
                  checked={prefs.reduceMotion}
                  onCheckedChange={(v) => onUpdatePrefs({ reduceMotion: v })}
                />
              </div>
            </div>
          )}

          {section === "fonts" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div className="flex items-start justify-between gap-2">
                <div>
                  <h2 className="text-base font-semibold">开源字体与双排版系统</h2>
                  <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                    全部预设采用 SIL OFL 开源免费字体；支持独立设置界面字体与代码块字体，或一键跟随。
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    onUpdatePrefs({
                      uiFontPreset: DEFAULT_PREFERENCES.uiFontPreset,
                      codeFontPreset: DEFAULT_PREFERENCES.codeFontPreset,
                      codeFollowsUi: false,
                      uiFontSize: 14,
                      codeFontSize: 13,
                      codeLigatures: true,
                    })
                  }
                >
                  恢复默认
                </Button>
              </div>

              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5 flex flex-col gap-3.5">
                <div className="grid grid-cols-1 sm:grid-cols-[1fr_130px] gap-3 items-end">
                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium">界面字体（UI &amp; 对话正文）</label>
                    <Select
                      value={prefs.uiFontPreset}
                      onValueChange={(v) => onUpdatePrefs({ uiFontPreset: v as UiFontPresetId })}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {UI_FONT_PRESETS.map((f) => (
                          <SelectItem key={f.id} value={f.id}>
                            {f.name} · {f.license}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium">UI 字号</label>
                    <div className="flex items-center justify-between rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-control)] h-9 px-2">
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer hover:text-[var(--soxu-text-primary)]"
                        onClick={() => onUpdatePrefs({ uiFontSize: Math.max(12, prefs.uiFontSize - 1) })}
                        aria-label="减小 UI 字号"
                      >
                        -
                      </button>
                      <span className="text-xs soxu-mono soxu-tabular">{prefs.uiFontSize}px</span>
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer hover:text-[var(--soxu-text-primary)]"
                        onClick={() => onUpdatePrefs({ uiFontSize: Math.min(17, prefs.uiFontSize + 1) })}
                        aria-label="增大 UI 字号"
                      >
                        +
                      </button>
                    </div>
                  </div>
                </div>

                <Separator />

                <div className="flex items-center justify-between gap-4">
                  <div>
                    <div className="text-xs font-medium">代码块字体跟随界面字体</div>
                    <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                      开启后代码预览、Diff 与终端统一继承当前界面字体（{activeUiPreset.name}）
                    </div>
                  </div>
                  <Switch
                    checked={prefs.codeFollowsUi}
                    onCheckedChange={(v) => onUpdatePrefs({ codeFollowsUi: v })}
                  />
                </div>

                <div
                  className={cn(
                    "grid grid-cols-1 sm:grid-cols-[1fr_130px] gap-3 items-end transition-opacity duration-150",
                    prefs.codeFollowsUi && "opacity-55",
                  )}
                >
                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium flex items-center justify-between">
                      <span>代码块字体（Diff &amp; 终端等宽）</span>
                      {prefs.codeFollowsUi && <Badge variant="accent">当前已跟随界面字体</Badge>}
                    </label>
                    <Select
                      disabled={prefs.codeFollowsUi}
                      value={prefs.codeFontPreset}
                      onValueChange={(v) => onUpdatePrefs({ codeFontPreset: v as CodeFontPresetId })}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {CODE_FONT_PRESETS.map((f) => (
                          <SelectItem key={f.id} value={f.id}>
                            {f.name} · {f.license}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>

                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium">代码字号</label>
                    <div className="flex items-center justify-between rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-control)] h-9 px-2">
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer"
                        onClick={() => onUpdatePrefs({ codeFontSize: Math.max(11, prefs.codeFontSize - 1) })}
                        aria-label="减小代码字号"
                      >
                        -
                      </button>
                      <span className="text-xs soxu-mono soxu-tabular">{prefs.codeFontSize}px</span>
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer"
                        onClick={() => onUpdatePrefs({ codeFontSize: Math.min(17, prefs.codeFontSize + 1) })}
                        aria-label="增大代码字号"
                      >
                        +
                      </button>
                    </div>
                  </div>
                </div>

                <div className="flex items-center justify-between gap-4">
                  <div>
                    <div className="text-xs font-medium">启用编程连字（!=、=&gt;、::、-&gt;）</div>
                    <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                      控制 CSS font-feature-settings 与 GPUI 文本塑形连字开关
                    </div>
                  </div>
                  <Switch
                    checked={prefs.codeLigatures}
                    onCheckedChange={(v) => onUpdatePrefs({ codeLigatures: v })}
                  />
                </div>
              </div>

              {/* 双语 + Rust 代码实时渲染预览 */}
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-canvas)] p-3.5 flex flex-col gap-2">
                <div className="flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)]">
                  <span>实时排版预览 · UI: {activeUiPreset.name}</span>
                  <span className="soxu-mono">
                    Code: {prefs.codeFollowsUi ? `跟随 (${activeUiPreset.id})` : activeCodePreset.name}
                  </span>
                </div>
                <p className="m-0 leading-relaxed text-[var(--soxu-text-primary)]" style={{ fontSize: `${prefs.uiFontSize}px` }}>
                  Soxu 多 Agent 编排工作台支持 0123456789 等宽数字对齐与清晰的中文可读性。
                </p>
                <pre className="m-0 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-2.5 overflow-x-auto soxu-code-block">
                  <code>
                    <span className="syn-kw">pub const fn</span> <span className="syn-fn">supports_steer</span>(k:{" "}
                    <span className="syn-ty">Cli</span>) -&gt; <span className="syn-ty">bool</span> {"{"}{" "}
                    <span className="syn-mac">matches!</span>(k, Pi | Codex) &amp;&amp; k != Qwen {"}"}
                  </code>
                </pre>
              </div>
            </div>
          )}

          {section === "magpie" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div>
                  <h2 className="text-base font-semibold">Agent 驱动与 Magpie 路由拓扑</h2>
                  <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5 flex items-center gap-1.5">
                    <Lock className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                    <span>只读扫描本机配置，绝不修改 ~/.claude / ~/.codex / ~/.pi / ~/.config/magpie</span>
                  </p>
                </div>
                <Button size="sm" variant="default" onClick={onToggleMagpie}>
                  切换模拟：{magpieMode === "magpie_managed" ? "Magpie 协同" : "独立原生"}
                </Button>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3">
                  <div className="text-[11px] text-[var(--soxu-text-tertiary)]">模型网关</div>
                  <div className="text-xs font-semibold soxu-mono mt-1">
                    {magpieMode === "magpie_managed" ? "127.0.0.1:3425/v1 · 在线 (2ms)" : "已降级为各 CLI 原生直连"}
                  </div>
                </div>
                <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3">
                  <div className="text-[11px] text-[var(--soxu-text-tertiary)]">Worktree Direct MCP</div>
                  <div className="text-xs font-semibold soxu-mono mt-1">aoci · codegraph（启动钩子自动绑定）</div>
                </div>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                {CLI_OPTIONS.map((c) => (
                  <div
                    key={c.id}
                    className={cn(
                      "rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3 py-2.5 flex items-center justify-between",
                      !c.available && "opacity-50",
                    )}
                  >
                    <div>
                      <div className="text-xs font-semibold soxu-mono">{c.id}</div>
                      <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)]">bin: {c.bin}</div>
                    </div>
                    <Badge variant={c.available ? "accent" : "neutral"}>{c.available ? `v${c.ver}` : "未安装"}</Badge>
                  </div>
                ))}
              </div>
            </div>
          )}

          {section === "git" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">工作区、Worktree 与 Git 检查点</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                  隔离工作区共享外置构建缓存（CARGO_TARGET_DIR），大幅加快 Rust 增量编译。
                </p>
              </div>
              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">每轮对话自动生成 Git Checkpoint</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    允许任意时刻安全分叉或回退代码至指定回合，并自动保存备份 Stash
                  </div>
                </div>
                <Switch
                  checked={prefs.autoCheckpoint}
                  onCheckedChange={(v) => onUpdatePrefs({ autoCheckpoint: v })}
                />
              </div>
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5 flex items-center justify-between">
                <div>
                  <div className="text-xs font-medium">默认 Worktree 基准分支</div>
                  <div className="text-[11.5px] soxu-mono text-[var(--soxu-text-tertiary)] mt-0.5">
                    {prefs.defaultBranch}
                  </div>
                </div>
                <Badge variant="neutral">已绑定 Git 门禁</Badge>
              </div>
            </div>
          )}

          {section === "security" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">安全红线与审批门禁</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                  继承 AGENTS.md 最高优先级安全红线，高危命令默认锁定人工审慎确认。
                </p>
              </div>
              <div className="rounded-xl border border-[var(--soxu-semantic-danger-border)] bg-[var(--soxu-semantic-danger-soft)] p-3.5 text-xs flex flex-col gap-1.5">
                <div className="font-semibold text-[var(--soxu-semantic-danger)]">已强制锁定的保护规则</div>
                <div>• 递归删除、强制覆盖、push --force 等命令禁用「本会话始终允许」</div>
                <div>• 默认焦点强制停留在「拒绝并指导修改」按钮</div>
                <div>• 本地 .githooks + Gitleaks 阻止凭据与硬编码个人用户目录提交</div>
              </div>
            </div>
          )}

          {section === "mobile" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">移动遥控与 #offer= 一次性配对</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                  手机扫码自动建连并在浏览器清除地址栏令牌片段，防止截屏泄露。
                </p>
              </div>
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4 flex flex-wrap items-center gap-4">
                <div className="bg-white p-3 rounded-xl text-black flex items-center justify-center">
                  <QrCode className="h-24 w-24" strokeWidth={1.5} />
                </div>
                <div className="flex-1 min-w-[180px] flex flex-col gap-2">
                  <div className="text-xs text-[var(--soxu-text-tertiary)]">6 位动态配对码（5 分钟有效）</div>
                  <div className="text-2xl font-bold soxu-mono tracking-[0.18em]">{pairCode}</div>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      onClick={() => {
                        const next = `${Math.floor(100 + Math.random() * 900)} ${Math.floor(100 + Math.random() * 900)}`;
                        setPairCode(next);
                        onFlashToast(`已刷新配对码：${next}`);
                      }}
                    >
                      <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.75} />
                      <span>刷新配对码</span>
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {section === "about" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">快捷键与关于 Soxu</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-0.5">
                  Soxu 协议版本 v1 · Rust GPUI + Daemon Embedded React 19 / shadcn/ui PWA
                </p>
              </div>
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] divide-y divide-[var(--soxu-border-hairline)] text-xs">
                {[
                  ["打开全局命令面板", "Ctrl+K / Cmd+K"],
                  ["打开系统设置界面", "Ctrl+, / Cmd+,"],
                  ["折叠 / 展开左侧工作区栏", "Ctrl+B / Cmd+B"],
                  ["折叠 / 展开右侧审阅栏", "Ctrl+J / Cmd+J"],
                  ["运行中直接注入 Steer 纠偏", "Ctrl+Enter"],
                ].map(([label, key]) => (
                  <div key={label} className="px-3.5 py-2 flex items-center justify-between">
                    <span className="text-[var(--soxu-text-secondary)]">{label}</span>
                    <kbd className="soxu-mono text-[11px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)]">
                      {key}
                    </kbd>
                  </div>
                ))}
              </div>

              {onOpenOnboarding && (
                <Button
                  variant="outline"
                  className="self-start"
                  onClick={() => {
                    onOpenChange(false);
                    onOpenOnboarding();
                  }}
                >
                  <Sparkles className="h-3.5 w-3.5" strokeWidth={1.75} />
                  <span>重新运行 3 步首次使用引导</span>
                </Button>
              )}

              <Collapsible open={tokensOpen} onOpenChange={setTokensOpen}>
                <CollapsibleTrigger asChild>
                  <button
                    type="button"
                    className="w-full rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 text-xs font-medium flex items-center justify-between cursor-pointer hover:border-[var(--soxu-border-strong)]"
                  >
                    <span>开发者 Pure Black 令牌自检表（7 级中性灰阶）</span>
                    <ChevronDown
                      className={cn("h-4 w-4 transition-transform duration-150", tokensOpen && "rotate-180")}
                      strokeWidth={1.75}
                    />
                  </button>
                </CollapsibleTrigger>
                <CollapsibleContent className="pt-2">
                  <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                    {[
                      ["Canvas #000000", "--soxu-surface-canvas", "17.8:1"],
                      ["Sidebar #0D0D0D", "--soxu-surface-sidebar", "15.9:1"],
                      ["Card #171717", "--soxu-surface-card", "14.9:1"],
                      ["Raised #212121", "--soxu-surface-raised", "13.2:1"],
                      ["Floating #262626", "--soxu-surface-floating", "12.4:1"],
                      ["Control #2F2F2F", "--soxu-surface-control", "11.1:1"],
                    ].map(([name, cssVar, ratio]) => (
                      <div
                        key={name}
                        className="rounded-lg border border-[var(--soxu-border-subtle)] p-2.5 text-[11px]"
                        style={{ backgroundColor: `var(${cssVar})` }}
                      >
                        <div className="font-semibold soxu-mono">{name}</div>
                        <div className="text-[var(--soxu-semantic-accent)] mt-0.5">{ratio} AAA</div>
                      </div>
                    ))}
                  </div>
                </CollapsibleContent>
              </Collapsible>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
