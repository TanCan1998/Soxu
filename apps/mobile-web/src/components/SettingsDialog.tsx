import {
  Check,
  ChevronDown,
  Cpu,
  GitBranch,
  Globe,
  Keyboard,
  Lock,
  Monitor,
  Moon,
  Palette,
  QrCode,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  Smartphone,
  Sparkles,
  Sun,
  Trash2,
  Type,
} from "lucide-react";
import { useEffect, useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { CodeSnippetView } from "@/components/CodeBlockView";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { useI18n, type I18nKey } from "@/lib/i18n";
import {
  ACCENT_PRESETS,
  CODE_FONT_PRESETS,
  DEFAULT_PREFERENCES,
  UI_FONT_PRESETS,
  normalizeHexColor,
  resolveAccentHex,
  resolveThemeColor,
  type AccentPresetId,
  type CodeFontPresetId,
  type FoldPreviewMode,
  type LineHeightMode,
  type SoxuPreferences,
  type ThemeMode,
  type TimestampFormatMode,
  type UiFontPresetId,
  type WindowChromeStyle,
  type WorktreeLocationMode,
} from "@/lib/preferences";
import { detectMacPlatform, formatShortcut } from "@/lib/useAdaptiveViewport";
import { cn } from "@/lib/utils";
import { CLI_OPTIONS } from "@/mock-data";
import type { PermissionLevel } from "@/types";

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

const SECTIONS: { id: SettingsSectionId; labelKey: I18nKey; icon: typeof Palette }[] = [
  { id: "appearance", labelKey: "settings.section.appearance", icon: Palette },
  { id: "fonts", labelKey: "settings.section.fonts", icon: Type },
  { id: "magpie", labelKey: "settings.section.magpie", icon: Cpu },
  { id: "git", labelKey: "settings.section.git", icon: GitBranch },
  { id: "security", labelKey: "settings.section.security", icon: ShieldAlert },
  { id: "mobile", labelKey: "settings.section.mobile", icon: Smartphone },
  { id: "about", labelKey: "settings.section.about", icon: Keyboard },
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
  const { locales, t } = useI18n();
  const [section, setSection] = useState<SettingsSectionId>(initialSection);
  const [tokensOpen, setTokensOpen] = useState(false);
  const [customHexDraft, setCustomHexDraft] = useState(prefs.customAccentHex);
  const [pairCode, setPairCode] = useState("849 217");
  const [devices, setDevices] = useState([
    { id: "dev-1", name: "个人手机 · Safari 移动端", lastActive: "2 分钟前", ip: "192.168.1.108" },
    { id: "dev-2", name: "平板工作终端", lastActive: "昨天 21:14", ip: "192.168.1.119" },
  ]);

  useEffect(() => {
    if (open) setSection(initialSection);
  }, [open, initialSection]);

  useEffect(() => {
    setCustomHexDraft(prefs.customAccentHex);
  }, [prefs.customAccentHex]);

  const activeUiPreset = UI_FONT_PRESETS.find((x) => x.id === prefs.uiFontPreset) ?? UI_FONT_PRESETS[0];
  const activeCodePreset = CODE_FONT_PRESETS.find((x) => x.id === prefs.codeFontPreset) ?? CODE_FONT_PRESETS[0];
  const resolvedMode = resolveThemeColor(prefs.themeMode);
  const activeAccentHex = resolveAccentHex(prefs, resolvedMode);
  const isMac = detectMacPlatform(prefs.windowChromeStyle);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(880px,95vw)] h-[min(660px,90dvh)] p-0 overflow-hidden flex flex-col md:flex-row">
        <DialogHeader className="sr-only">
          <DialogTitle>{t("settings.title")}</DialogTitle>
          <DialogDescription>{t("settings.description")}</DialogDescription>
        </DialogHeader>

        {/* 左侧分区导航 */}
        <nav
          aria-label={t("settings.title")}
          className="md:w-[220px] shrink-0 bg-[var(--soxu-surface-sidebar)] border-b md:border-b-0 md:border-r border-[var(--soxu-border-hairline)] p-3 flex md:flex-col gap-1 overflow-x-auto md:overflow-y-auto soxu-auto-scrollbar"
        >
          <div className="hidden md:flex items-center justify-between px-2 pb-2.5 mb-1 border-b border-[var(--soxu-border-hairline)]">
            <span className="text-xs font-semibold tracking-wide">{t("settings.title")}</span>
            <kbd className="soxu-mono text-[10px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-tertiary)]">
              {formatShortcut("Mod+,", isMac)}
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
                <span>{t(s.labelKey)}</span>
              </button>
            );
          })}
        </nav>

        {/* 右侧设置详细配置 */}
        <div className="flex-1 overflow-y-auto p-5 sm:p-6 flex flex-col gap-5 soxu-scroll">
          {section === "appearance" && (
            <div className="flex flex-col gap-5 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">{t("appearance.heading")}</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">{t("appearance.subheading")}</p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                {(
                  [
                    {
                      id: "dark",
                      title: t("appearance.themeDarkTitle"),
                      desc: t("appearance.themeDarkDesc"),
                      icon: Moon,
                      bg: "#000000",
                      fg: "#ececec",
                    },
                    {
                      id: "light",
                      title: t("appearance.themeLightTitle"),
                      desc: t("appearance.themeLightDesc"),
                      icon: Sun,
                      bg: "#ffffff",
                      fg: "#0d0d0d",
                    },
                    {
                      id: "system",
                      title: t("appearance.themeSystemTitle"),
                      desc: t("appearance.themeSystemDesc"),
                      icon: Monitor,
                      bg: "#171717",
                      fg: "#ececec",
                    },
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
                        <span className="text-[10px] soxu-mono">Aa 中</span>
                      </div>
                      <div>
                        <div className="text-xs font-semibold">{item.title}</div>
                        <div className="text-[11px] text-[var(--soxu-text-tertiary)]">{item.desc}</div>
                      </div>
                    </button>
                  );
                })}
              </div>

              {/* 界面语言（可扩展国际化注册表） */}
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-xs font-medium flex items-center gap-1.5">
                    <Globe className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={1.75} />
                    <span>{t("appearance.languageLabel")}</span>
                  </div>
                  <div className="text-[11px] text-[var(--soxu-text-tertiary)] mt-0.5">{t("appearance.languageHint")}</div>
                </div>
                <Select
                  value={prefs.locale}
                  onValueChange={(nextLocale) => {
                    const matched = locales.find((l) => l.id === nextLocale);
                    onUpdatePrefs({ locale: nextLocale });
                    onFlashToast(t("appearance.localeChangedToast", { label: matched?.label ?? nextLocale }));
                  }}
                >
                  <SelectTrigger className="w-full sm:w-[210px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {locales.map((pack) => (
                      <SelectItem key={pack.id} value={pack.id}>
                        {pack.label}
                        {pack.nativeNote ? ` (${pack.nativeNote})` : ""}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* 主题强调色（预设色卡 + 自定义取色器与 Hex 输入） */}
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5 flex flex-col gap-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <div className="text-xs font-medium">{t("appearance.accentHeading")}</div>
                    <div className="text-[11px] text-[var(--soxu-text-tertiary)] mt-0.5">
                      {t("appearance.accentSubheading")}
                    </div>
                  </div>
                  <span
                    className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium border"
                    style={{
                      color: activeAccentHex,
                      backgroundColor: "var(--soxu-semantic-accent-soft)",
                      borderColor: "var(--soxu-semantic-accent-border)",
                    }}
                  >
                    <span className="h-2 w-2 rounded-full" style={{ backgroundColor: activeAccentHex }} />
                    <span>{t("appearance.accentPreviewTag")}</span>
                    <span className="soxu-mono">{activeAccentHex.toUpperCase()}</span>
                  </span>
                </div>

                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                  {ACCENT_PRESETS.map((preset) => {
                    const swatch = resolvedMode === "dark" ? preset.darkHex : preset.lightHex;
                    const active = prefs.accentPreset === preset.id;
                    const presetLabel = t(preset.labelKey);
                    return (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => {
                          onUpdatePrefs({ accentPreset: preset.id });
                          onFlashToast(t("appearance.accentChangedToast", { label: presetLabel }));
                        }}
                        className={cn(
                          "rounded-lg border px-2.5 py-2 flex items-center justify-between gap-2 text-xs cursor-pointer transition-colors",
                          active
                            ? "bg-[var(--soxu-surface-raised)] border-[var(--soxu-border-focus-ring)] text-[var(--soxu-text-primary)] font-medium"
                            : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-strong)]",
                        )}
                      >
                        <span className="flex items-center gap-2 min-w-0">
                          <span
                            className="h-3.5 w-3.5 rounded-full shrink-0 border border-black/20"
                            style={{ backgroundColor: swatch }}
                          />
                          <span className="truncate">{presetLabel}</span>
                        </span>
                        {active && <Check className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                      </button>
                    );
                  })}

                  <button
                    type="button"
                    onClick={() => {
                      const valid = normalizeHexColor(customHexDraft, prefs.customAccentHex);
                      onUpdatePrefs({ accentPreset: "custom", customAccentHex: valid });
                    }}
                    className={cn(
                      "rounded-lg border px-2.5 py-2 flex items-center justify-between gap-2 text-xs cursor-pointer transition-colors",
                      prefs.accentPreset === "custom"
                        ? "bg-[var(--soxu-surface-raised)] border-[var(--soxu-border-focus-ring)] text-[var(--soxu-text-primary)] font-medium"
                        : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-strong)]",
                    )}
                  >
                    <span className="flex items-center gap-2 min-w-0">
                      <span
                        className="h-3.5 w-3.5 rounded-full shrink-0 border border-white/25"
                        style={{ backgroundColor: normalizeHexColor(prefs.customAccentHex) }}
                      />
                      <span className="truncate">{t("appearance.accentCustom")}</span>
                    </span>
                    {prefs.accentPreset === "custom" && (
                      <Check className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />
                    )}
                  </button>
                </div>

                <div className="flex flex-wrap items-center gap-2 pt-0.5">
                  <label className="text-[11.5px] text-[var(--soxu-text-secondary)]" htmlFor="soxu-custom-accent-picker">
                    {t("appearance.accentCustomInput")}
                  </label>
                  <input
                    id="soxu-custom-accent-picker"
                    type="color"
                    value={normalizeHexColor(prefs.customAccentHex)}
                    onChange={(e) => {
                      const hex = normalizeHexColor(e.target.value);
                      setCustomHexDraft(hex);
                      onUpdatePrefs({ accentPreset: "custom" as AccentPresetId, customAccentHex: hex });
                    }}
                    className="h-8 w-10 rounded-md border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-control)] p-0.5 cursor-pointer"
                    aria-label={t("appearance.accentCustomInput")}
                  />
                  <input
                    type="text"
                    value={customHexDraft}
                    onChange={(e) => {
                      const val = e.target.value;
                      setCustomHexDraft(val);
                      if (/^#?[0-9a-fA-F]{6}$/.test(val.trim())) {
                        onUpdatePrefs({
                          accentPreset: "custom",
                          customAccentHex: normalizeHexColor(val),
                        });
                      }
                    }}
                    onBlur={() => {
                      const valid = normalizeHexColor(customHexDraft, prefs.customAccentHex);
                      setCustomHexDraft(valid);
                    }}
                    placeholder="#6366f1"
                    maxLength={7}
                    className="h-8 w-28 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-composer)] px-2.5 text-xs soxu-mono text-[var(--soxu-text-primary)] outline-none focus:border-[var(--soxu-border-focus-ring)]"
                  />
                </div>
              </div>

              <Separator />

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">{t("appearance.lineHeight")}</label>
                  <Select
                    value={prefs.uiLineHeight}
                    onValueChange={(v) => onUpdatePrefs({ uiLineHeight: v as LineHeightMode })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="compact">{t("appearance.lineHeightCompact")}</SelectItem>
                      <SelectItem value="normal">{t("appearance.lineHeightNormal")}</SelectItem>
                      <SelectItem value="relaxed">{t("appearance.lineHeightRelaxed")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">{t("appearance.timestampFormat")}</label>
                  <Select
                    value={prefs.timestampFormat}
                    onValueChange={(v) => onUpdatePrefs({ timestampFormat: v as TimestampFormatMode })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="relative">{t("appearance.timestampRelative")}</SelectItem>
                      <SelectItem value="absolute">{t("appearance.timestampAbsolute")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="flex items-center justify-between gap-4">
                <div>
                  <div className="text-xs font-medium">{t("appearance.densityTitle")}</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">{t("appearance.densityDesc")}</div>
                </div>
                <Switch
                  checked={prefs.density === "compact"}
                  onCheckedChange={(v) => onUpdatePrefs({ density: v ? "compact" : "comfortable" })}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div>
                  <div className="text-xs font-medium">{t("appearance.reduceMotionTitle")}</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">{t("appearance.reduceMotionDesc")}</div>
                </div>
                <Switch checked={prefs.reduceMotion} onCheckedChange={(v) => onUpdatePrefs({ reduceMotion: v })} />
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">{t("appearance.windowChromeTitle")}</label>
                  <Select
                    value={prefs.windowChromeStyle ?? "auto"}
                    onValueChange={(v) => onUpdatePrefs({ windowChromeStyle: v as WindowChromeStyle })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t("appearance.windowChromeAuto")}</SelectItem>
                      <SelectItem value="macos">{t("appearance.windowChromeMac")}</SelectItem>
                      <SelectItem value="windows">{t("appearance.windowChromeWin")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">{t("appearance.foldPreviewTitle")}</label>
                  <Select
                    value={prefs.foldPreviewMode ?? "auto"}
                    onValueChange={(v) => onUpdatePrefs({ foldPreviewMode: v as FoldPreviewMode })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">{t("appearance.foldPreviewAuto")}</SelectItem>
                      <SelectItem value="dual-horizontal">{t("appearance.foldPreviewDualH")}</SelectItem>
                      <SelectItem value="dual-vertical">{t("appearance.foldPreviewDualV")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 flex items-center justify-between gap-3">
                <div className="text-xs text-[var(--soxu-text-secondary)]">
                  {t("appearance.sidebarWidths", {
                    left: prefs.leftSidebarWidth,
                    right: prefs.rightInspectorWidth,
                  })}
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    onUpdatePrefs({
                      leftSidebarWidth: DEFAULT_PREFERENCES.leftSidebarWidth,
                      rightInspectorWidth: DEFAULT_PREFERENCES.rightInspectorWidth,
                    });
                    onFlashToast(t("appearance.resetWidthsToast"));
                  }}
                >
                  <RotateCcw className="h-3.5 w-3.5" strokeWidth={1.75} />
                  <span>{t("appearance.resetWidths")}</span>
                </Button>
              </div>
            </div>
          )}

          {section === "fonts" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div className="flex items-start justify-between gap-2">
                <div>
                  <h2 className="text-base font-semibold">开源字体与代码排版细调</h2>
                  <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">
                    全部预设基于开源免费字体；支持独立设定界面与代码字体，或一键同步跟随。
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
                      showLineNumbers: true,
                      codeWordWrap: false,
                      tabSize: 2,
                    })
                  }
                >
                  恢复默认排版
                </Button>
              </div>

              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4 flex flex-col gap-3.5">
                <div className="grid grid-cols-1 sm:grid-cols-[1fr_130px] gap-3 items-end">
                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium">界面字体（菜单与对话正文）</label>
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
                    <label className="text-xs font-medium">界面字号</label>
                    <div className="flex items-center justify-between rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-control)] h-9 px-2">
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer hover:text-[var(--soxu-text-primary)]"
                        onClick={() => onUpdatePrefs({ uiFontSize: Math.max(12, prefs.uiFontSize - 1) })}
                        aria-label="减小界面字号"
                      >
                        -
                      </button>
                      <span className="text-xs soxu-mono soxu-tabular">{prefs.uiFontSize}px</span>
                      <button
                        type="button"
                        className="px-2 text-sm cursor-pointer hover:text-[var(--soxu-text-primary)]"
                        onClick={() => onUpdatePrefs({ uiFontSize: Math.min(17, prefs.uiFontSize + 1) })}
                        aria-label="增大界面字号"
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
                      开启后代码框、变更对比与终端统一继承当前界面字体（{activeUiPreset.name}）
                    </div>
                  </div>
                  <Switch checked={prefs.codeFollowsUi} onCheckedChange={(v) => onUpdatePrefs({ codeFollowsUi: v })} />
                </div>

                <div
                  className={cn(
                    "grid grid-cols-1 sm:grid-cols-[1fr_130px] gap-3 items-end transition-opacity duration-150",
                    prefs.codeFollowsUi && "opacity-55",
                  )}
                >
                  <div className="flex flex-col gap-1.5">
                    <label className="text-xs font-medium flex items-center justify-between">
                      <span>代码块字体（源码与终端等宽）</span>
                      {prefs.codeFollowsUi && <Badge variant="accent">已跟随界面字体</Badge>}
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

                <Separator />

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-xs font-medium">显示代码块行号</div>
                      <div className="text-[11px] text-[var(--soxu-text-tertiary)]">左侧固定右对齐唯一行号槽</div>
                    </div>
                    <Switch checked={prefs.showLineNumbers} onCheckedChange={(v) => onUpdatePrefs({ showLineNumbers: v })} />
                  </div>

                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-xs font-medium">长代码自动折行</div>
                      <div className="text-[11px] text-[var(--soxu-text-tertiary)]">关闭则保留横向滚动条对齐</div>
                    </div>
                    <Switch checked={prefs.codeWordWrap} onCheckedChange={(v) => onUpdatePrefs({ codeWordWrap: v })} />
                  </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 items-center">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-xs font-medium">启用编程连字（!=、=&gt;、-&gt;）</div>
                      <div className="text-[11px] text-[var(--soxu-text-tertiary)]">合并多字符操作符为连体字形</div>
                    </div>
                    <Switch checked={prefs.codeLigatures} onCheckedChange={(v) => onUpdatePrefs({ codeLigatures: v })} />
                  </div>

                  <div className="flex items-center justify-between gap-3">
                    <span className="text-xs font-medium">制表符缩进宽度</span>
                    <Select
                      value={String(prefs.tabSize)}
                      onValueChange={(v) => onUpdatePrefs({ tabSize: Number(v) as 2 | 4 })}
                    >
                      <SelectTrigger className="w-28 h-8">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="2">2 个空格</SelectItem>
                        <SelectItem value="4">4 个空格</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              </div>

              {/* 双语与代码实时预览 */}
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-canvas)] p-3.5 flex flex-col gap-2.5">
                <div className="flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)]">
                  <span>实时效果预览：界面使用 {activeUiPreset.name}</span>
                  <span className="soxu-mono">
                    代码：{prefs.codeFollowsUi ? "跟随界面字体" : activeCodePreset.name}
                  </span>
                </div>
                <p className="m-0 leading-relaxed text-[var(--soxu-text-primary)]" style={{ fontSize: `${prefs.uiFontSize}px` }}>
                  多代理协同工作台支持 0123456789 等宽数字对齐与清晰的中文混排可读性。
                </p>
                <CodeSnippetView
                  startLine={22}
                  showLineNumbers={prefs.showLineNumbers}
                  wrap={prefs.codeWordWrap}
                  lines={[
                    "pub const fn supports_live_steer(kind: NativeCliKind) -> bool {",
                    "  matches!(kind, NativeCliKind::Pi | NativeCliKind::Codex)",
                    "}",
                  ]}
                />
              </div>
            </div>
          )}

          {section === "magpie" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div>
                  <h2 className="text-base font-semibold">代理驱动与路由器发现</h2>
                  <p className="text-xs text-[var(--soxu-text-secondary)] mt-1 flex items-center gap-1.5">
                    <Lock className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                    <span>仅只读扫描本机配置文件，绝不篡改用户主目录下的外部配置</span>
                  </p>
                </div>
                <Button size="sm" variant="default" onClick={onToggleMagpie}>
                  切换模拟：{magpieMode === "magpie_managed" ? "协同托管模式" : "原生直连模式"}
                </Button>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                  <div className="text-[11px] text-[var(--soxu-text-tertiary)]">模型网关通道</div>
                  <div className="text-xs font-semibold soxu-mono mt-1">
                    {magpieMode === "magpie_managed" ? "127.0.0.1:3425/v1 · 延迟 2 毫秒" : "已切换为各驱动独立直连"}
                  </div>
                </div>
                <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                  <div className="text-[11px] text-[var(--soxu-text-tertiary)]">工作树工具上下文</div>
                  <div className="text-xs font-semibold soxu-mono mt-1">索引引擎与符号图自动按目录注入</div>
                </div>
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">后台服务健康探测周期</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">定时检查模型路由与本地原生命令行存活状态</div>
                </div>
                <Select
                  value={String(prefs.probeIntervalSec)}
                  onValueChange={(v) => onUpdatePrefs({ probeIntervalSec: Number(v) as 15 | 30 | 60 })}
                >
                  <SelectTrigger className="w-32 h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="15">每 15 秒</SelectItem>
                    <SelectItem value="30">每 30 秒</SelectItem>
                    <SelectItem value="60">每 60 秒</SelectItem>
                  </SelectContent>
                </Select>
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
                    <div className="flex items-center gap-2.5 min-w-0">
                      <AgentIcon agent={c.id} className="h-4 w-4 shrink-0" />
                      <div>
                        <div className="text-xs font-semibold soxu-mono">{c.id}</div>
                        <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)]">命令名: {c.bin}</div>
                      </div>
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
                <h2 className="text-base font-semibold">工作区目录、隔离工作树与提交检查点</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">
                  自定义基准分支、工作树存放策略以及防丢失回退检查点。
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4">
                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium" htmlFor="pref-default-branch">
                    默认拉取与对比基准分支
                  </label>
                  <input
                    id="pref-default-branch"
                    value={prefs.defaultBranch}
                    onChange={(e) => onUpdatePrefs({ defaultBranch: e.target.value })}
                    className="h-9 rounded-lg border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] px-3 text-xs soxu-mono text-[var(--soxu-text-primary)] outline-none focus:border-[var(--soxu-border-focus-ring)]"
                  />
                </div>
                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">新工作树存储位置</label>
                  <Select
                    value={prefs.worktreeLocation}
                    onValueChange={(v) => onUpdatePrefs({ worktreeLocation: v as WorktreeLocationMode })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="sibling">隔离池集中管理 (.worktrees/)</SelectItem>
                      <SelectItem value="central">主仓库同级平行目录</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">每轮对话自动保存代码检查点</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    支持随时安全分叉或无损回退至任意回合，自动备份未提交修改
                  </div>
                </div>
                <Switch checked={prefs.autoCheckpoint} onCheckedChange={(v) => onUpdatePrefs({ autoCheckpoint: v })} />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">跨工作树共享 Rust 编译缓存目录</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    自动复用共享增量编译产物，新建并行工作树无需从头全量编译
                  </div>
                </div>
                <Switch checked={prefs.sharedCargoTarget} onCheckedChange={(v) => onUpdatePrefs({ sharedCargoTarget: v })} />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">关闭含未提交改动工作树前弹出确认</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    防止在对话归档或工作树清理时遗漏未保存变更
                  </div>
                </div>
                <Switch
                  checked={prefs.warnUncommittedOnClose}
                  onCheckedChange={(v) => onUpdatePrefs({ warnUncommittedOnClose: v })}
                />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">提交标题默认启用表情分类前缀</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    严格对齐仓库提交门禁格式要求
                  </div>
                </div>
                <Switch checked={prefs.commitEmojiPrefix} onCheckedChange={(v) => onUpdatePrefs({ commitEmojiPrefix: v })} />
              </div>
            </div>
          )}

          {section === "security" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">操作权限默认策略与安全红线</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">
                  控制新会话初始授权策略、终端执行超时保护以及最高优先级破坏性命令锁定规则。
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4">
                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">新建会话默认操作权限</label>
                  <Select
                    value={prefs.defaultPermMode}
                    onValueChange={(v) => onUpdatePrefs({ defaultPermMode: v as PermissionLevel })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="plan">只读规划（仅分析不写盘）</SelectItem>
                      <SelectItem value="ask">每次询问确认（推荐默认）</SelectItem>
                      <SelectItem value="auto-edit">自动编辑（仅工作区内写入放行）</SelectItem>
                      <SelectItem value="full-access">完全授权（仅高危拦截）</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="flex flex-col gap-1.5">
                  <label className="text-xs font-medium">受控终端单条命令超时限制</label>
                  <Select
                    value={String(prefs.commandTimeoutSec)}
                    onValueChange={(v) => onUpdatePrefs({ commandTimeoutSec: Number(v) as 30 | 120 | 300 | 0 })}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="30">30 秒</SelectItem>
                      <SelectItem value="120">120 秒（推荐）</SelectItem>
                      <SelectItem value="300">5 分钟</SelectItem>
                      <SelectItem value="0">不限时长</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="flex items-center justify-between gap-4 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-3.5">
                <div>
                  <div className="text-xs font-medium">界面展示自动折叠用户主目录路径</div>
                  <div className="text-[11.5px] text-[var(--soxu-text-tertiary)]">
                    统一显示为波浪线缩写前缀，避免截屏分享时意外暴露操作系统用户名
                  </div>
                </div>
                <Switch checked={prefs.redactHomePath} onCheckedChange={(v) => onUpdatePrefs({ redactHomePath: v })} />
              </div>

              <div className="rounded-xl border border-[var(--soxu-semantic-danger-border)] bg-[var(--soxu-semantic-danger-soft)] p-4 text-xs flex flex-col gap-2">
                <div className="font-semibold text-[var(--soxu-semantic-danger)]">已强制锁定的最高优先级保护规则</div>
                <div>• 递归删除、强行覆盖工作树、强制推送分支等高危命令禁止自动放行</div>
                <div>• 审批触发后默认优先高亮拒绝并反馈，防止快捷键误触确认</div>
                <div>• 本地与云端密钥泄露扫描拦截个人路径及敏感凭据提交</div>
              </div>
            </div>
          )}

          {section === "mobile" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div>
                <h2 className="text-base font-semibold">移动终端遥控与配对安全</h2>
                <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">
                  手机扫码一次性令牌片段建立连接后自动清理地址栏凭据，防止截屏泄露。
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3.5 rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4 items-center">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <div className="text-xs font-medium">局域网内嵌服务端开关</div>
                    <div className="text-[11px] text-[var(--soxu-text-tertiary)]">允许同局域网移动设备直连审批与发令</div>
                  </div>
                  <Switch
                    checked={prefs.lanRemoteEnabled}
                    onCheckedChange={(v) => onUpdatePrefs({ lanRemoteEnabled: v })}
                  />
                </div>

                <div className="flex items-center justify-between gap-3">
                  <span className="text-xs font-medium">无操作自动锁定</span>
                  <Select
                    value={String(prefs.remoteIdleLockMin)}
                    onValueChange={(v) => onUpdatePrefs({ remoteIdleLockMin: Number(v) as 5 | 15 | 30 | 0 })}
                  >
                    <SelectTrigger className="w-32 h-8">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="5">5 分钟</SelectItem>
                      <SelectItem value="15">15 分钟</SelectItem>
                      <SelectItem value="30">30 分钟</SelectItem>
                      <SelectItem value="0">不自动锁定</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4 flex flex-wrap items-center gap-4">
                <div className="bg-white p-3 rounded-xl text-black flex items-center justify-center">
                  <QrCode className="h-24 w-24" strokeWidth={1.5} />
                </div>
                <div className="flex-1 min-w-[180px] flex flex-col gap-2">
                  <div className="text-xs text-[var(--soxu-text-tertiary)]">6 位一次性配对码（5 分钟有效）</div>
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

              <div className="flex flex-col gap-2">
                <div className="text-xs font-medium text-[var(--soxu-text-secondary)]">已授权配对设备 ({devices.length})</div>
                {devices.length === 0 ? (
                  <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-4 text-center text-xs text-[var(--soxu-text-tertiary)]">
                    当前暂无已配对移动设备
                  </div>
                ) : (
                  devices.map((dev) => (
                    <div
                      key={dev.id}
                      className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 flex items-center justify-between gap-2 text-xs"
                    >
                      <div>
                        <div className="font-medium text-[var(--soxu-text-primary)]">{dev.name}</div>
                        <div className="text-[11px] text-[var(--soxu-text-tertiary)] soxu-tabular">
                          最近活动：{dev.lastActive} · {dev.ip}
                        </div>
                      </div>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-[var(--soxu-semantic-danger)] hover:text-[var(--soxu-semantic-danger)]"
                        onClick={() => {
                          setDevices((prev) => prev.filter((x) => x.id !== dev.id));
                          onFlashToast(`已吊销设备授权：${dev.name}`);
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                        <span>移除</span>
                      </Button>
                    </div>
                  ))
                )}
              </div>
            </div>
          )}

          {section === "about" && (
            <div className="flex flex-col gap-4 animate-in fade-in-0 duration-150">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h2 className="text-base font-semibold">{t("about.heading")}</h2>
                  <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">
                    通信协议第 1 版 · 原生桌面引擎与内嵌响应式移动终端
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    onUpdatePrefs(DEFAULT_PREFERENCES);
                    onFlashToast("已恢复全部出厂默认设置");
                  }}
                >
                  全部恢复默认
                </Button>
              </div>

              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] divide-y divide-[var(--soxu-border-hairline)] text-xs">
                {[
                  ["打开全局命令搜索面板", formatShortcut("Mod+K", isMac)],
                  ["打开系统设置弹窗", formatShortcut("Mod+,", isMac)],
                  ["折叠或展开左侧栏", formatShortcut("Mod+B", isMac)],
                  ["折叠或展开右侧审阅栏", formatShortcut("Mod+J", isMac)],
                  ["切换操作权限档位", formatShortcut("Alt+P", isMac)],
                  ["发送当前指令（换行请用 Shift+Enter）", "Enter"],
                  ["运行中快速注入热纠偏指令", formatShortcut("Mod+Enter", isMac)],
                ].map(([label, key]) => (
                  <div key={label} className="px-3.5 py-2.5 flex items-center justify-between">
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
                  <span>按需重新运行三步初次使用引导</span>
                </Button>
              )}

              <Collapsible open={tokensOpen} onOpenChange={setTokensOpen}>
                <CollapsibleTrigger asChild>
                  <button
                    type="button"
                    className="w-full rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 text-xs font-medium flex items-center justify-between cursor-pointer hover:border-[var(--soxu-border-strong)]"
                  >
                    <span>纯黑七级灰阶表面对比度校验表</span>
                    <ChevronDown
                      className={cn("h-4 w-4 transition-transform duration-150", tokensOpen && "rotate-180")}
                      strokeWidth={1.75}
                    />
                  </button>
                </CollapsibleTrigger>
                <CollapsibleContent className="pt-2">
                  <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                    {[
                      ["主画布 #000000", "--soxu-surface-canvas", "对比度 17.8:1"],
                      ["侧边栏 #0D0D0D", "--soxu-surface-sidebar", "对比度 15.9:1"],
                      ["卡片底 #171717", "--soxu-surface-card", "对比度 14.9:1"],
                      ["抬升层 #212121", "--soxu-surface-raised", "对比度 13.2:1"],
                      ["浮窗底 #262626", "--soxu-surface-floating", "对比度 12.4:1"],
                      ["控件底 #2F2F2F", "--soxu-surface-control", "对比度 11.1:1"],
                    ].map(([name, cssVar, ratio]) => (
                      <div
                        key={name}
                        className="rounded-lg border border-[var(--soxu-border-subtle)] p-2.5 text-[11px]"
                        style={{ backgroundColor: `var(${cssVar})` }}
                      >
                        <div className="font-semibold soxu-mono">{name}</div>
                        <div className="text-[var(--soxu-semantic-accent)] mt-0.5">{ratio}</div>
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
