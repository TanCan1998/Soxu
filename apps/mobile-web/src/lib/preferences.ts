import type { PermissionLevel } from "@/types";

export type ThemeMode = "dark" | "light" | "system";
export type DensityMode = "compact" | "comfortable";
export type UiFontPresetId = "inter-noto" | "ibm-plex-noto" | "jakarta-noto" | "lxgw-wenkai" | "system-sans";
export type CodeFontPresetId = "jetbrains-mono" | "fira-code" | "maple-mono" | "ibm-plex-mono" | "cascadia-code";
export type LineHeightMode = "compact" | "normal" | "relaxed";
export type TimestampFormatMode = "relative" | "absolute";
export type WorktreeLocationMode = "sibling" | "central";

export interface FontPreset<T extends string> {
  id: T;
  name: string;
  license: string;
  stack: string;
}

export const UI_FONT_PRESETS: FontPreset<UiFontPresetId>[] = [
  {
    id: "inter-noto",
    name: "Inter 与思源黑体 (默认无衬线)",
    license: "SIL OFL 1.1",
    stack: "'Inter', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "ibm-plex-noto",
    name: "IBM Plex Sans 与思源黑体",
    license: "SIL OFL 1.1",
    stack: "'IBM Plex Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "jakarta-noto",
    name: "Plus Jakarta Sans 与思源黑体",
    license: "SIL OFL 1.1",
    stack: "'Plus Jakarta Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "lxgw-wenkai",
    name: "霞鹜文楷 (清晰手写风格)",
    license: "SIL OFL 1.1",
    stack: "'Inter', 'LXGW WenKai Screen', 'LXGW WenKai', 'Noto Sans SC', sans-serif",
  },
  {
    id: "system-sans",
    name: "系统原生无衬线字体",
    license: "系统内置",
    stack: "system-ui, -apple-system, 'PingFang SC', 'Microsoft YaHei UI', sans-serif",
  },
];

export const CODE_FONT_PRESETS: FontPreset<CodeFontPresetId>[] = [
  {
    id: "jetbrains-mono",
    name: "JetBrains Mono",
    license: "SIL OFL 1.1",
    stack: "'JetBrains Mono', 'Noto Sans SC', monospace",
  },
  {
    id: "fira-code",
    name: "Fira Code",
    license: "SIL OFL 1.1",
    stack: "'Fira Code', 'Noto Sans SC', monospace",
  },
  {
    id: "maple-mono",
    name: "Maple Mono (中英文严格等宽)",
    license: "SIL OFL 1.1",
    stack: "'Maple Mono NF CN', 'Maple Mono', 'JetBrains Mono', 'Noto Sans SC', monospace",
  },
  {
    id: "ibm-plex-mono",
    name: "IBM Plex Mono",
    license: "SIL OFL 1.1",
    stack: "'IBM Plex Mono', 'Noto Sans SC', monospace",
  },
  {
    id: "cascadia-code",
    name: "Cascadia Code",
    license: "SIL OFL 1.1",
    stack: "'Cascadia Code', 'JetBrains Mono', 'Noto Sans SC', monospace",
  },
];

export interface SoxuPreferences {
  themeMode: ThemeMode;
  density: DensityMode;
  uiLineHeight: LineHeightMode;
  timestampFormat: TimestampFormatMode;
  leftSidebarWidth: number;
  rightInspectorWidth: number;
  uiFontPreset: UiFontPresetId;
  codeFollowsUi: boolean;
  codeFontPreset: CodeFontPresetId;
  uiFontSize: number;
  codeFontSize: number;
  tabSize: 2 | 4;
  showLineNumbers: boolean;
  codeWordWrap: boolean;
  codeLigatures: boolean;
  reduceMotion: boolean;
  probeIntervalSec: 15 | 30 | 60;
  autoCheckpoint: boolean;
  sharedCargoTarget: boolean;
  warnUncommittedOnClose: boolean;
  commitEmojiPrefix: boolean;
  worktreeLocation: WorktreeLocationMode;
  defaultBranch: string;
  defaultPermMode: PermissionLevel;
  commandTimeoutSec: 30 | 120 | 300 | 0;
  redactHomePath: boolean;
  lanRemoteEnabled: boolean;
  remoteIdleLockMin: 5 | 15 | 30 | 0;
}

export const DEFAULT_PREFERENCES: SoxuPreferences = {
  themeMode: "dark",
  density: "compact",
  uiLineHeight: "normal",
  timestampFormat: "relative",
  leftSidebarWidth: 264,
  rightInspectorWidth: 380,
  uiFontPreset: "inter-noto",
  codeFollowsUi: false,
  codeFontPreset: "jetbrains-mono",
  uiFontSize: 14,
  codeFontSize: 13,
  tabSize: 2,
  showLineNumbers: true,
  codeWordWrap: false,
  codeLigatures: true,
  reduceMotion: false,
  probeIntervalSec: 30,
  autoCheckpoint: true,
  sharedCargoTarget: true,
  warnUncommittedOnClose: true,
  commitEmojiPrefix: true,
  worktreeLocation: "sibling",
  defaultBranch: "origin/main",
  defaultPermMode: "ask",
  commandTimeoutSec: 120,
  redactHomePath: true,
  lanRemoteEnabled: true,
  remoteIdleLockMin: 15,
};

const STORAGE_KEY = "soxu.preferences.v2";

export function loadPreferences(): SoxuPreferences {
  if (typeof window === "undefined") return DEFAULT_PREFERENCES;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_PREFERENCES;
    return { ...DEFAULT_PREFERENCES, ...(JSON.parse(raw) as Partial<SoxuPreferences>) };
  } catch {
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(prefs: SoxuPreferences): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // 隐私模式或静态预览环境下忽略持久化异常
  }
}

export function resolveThemeColor(mode: ThemeMode): "dark" | "light" {
  if (mode === "system" && typeof window !== "undefined") {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return mode === "light" ? "light" : "dark";
}

export function applyPreferencesToDom(prefs: SoxuPreferences): "dark" | "light" {
  const resolved = resolveThemeColor(prefs.themeMode);
  if (typeof document === "undefined") return resolved;

  const root = document.documentElement;
  root.setAttribute("data-theme", resolved);
  root.setAttribute("data-density", prefs.density);
  root.setAttribute("data-reduce-motion", String(prefs.reduceMotion));

  const uiStack = (UI_FONT_PRESETS.find((x) => x.id === prefs.uiFontPreset) ?? UI_FONT_PRESETS[0]).stack;
  const codeStack = (CODE_FONT_PRESETS.find((x) => x.id === prefs.codeFontPreset) ?? CODE_FONT_PRESETS[0]).stack;

  root.style.setProperty("--soxu-font-ui", uiStack);
  root.style.setProperty("--soxu-font-code", codeStack);
  root.style.setProperty("--soxu-font-code-effective", prefs.codeFollowsUi ? "var(--soxu-font-ui)" : "var(--soxu-font-code)");
  root.style.setProperty("--soxu-font-ui-size", `${prefs.uiFontSize}px`);
  root.style.setProperty("--soxu-font-code-size", `${prefs.codeFontSize}px`);
  root.style.setProperty("--soxu-code-ligatures", prefs.codeLigatures ? '"liga" 1, "calt" 1' : '"liga" 0, "calt" 0');
  root.style.setProperty(
    "--soxu-ui-line-height",
    prefs.uiLineHeight === "compact" ? "1.38" : prefs.uiLineHeight === "relaxed" ? "1.68" : "1.5",
  );
  root.style.setProperty("tab-size", String(prefs.tabSize));

  const meta = document.getElementById("soxu-theme-meta");
  if (meta) {
    meta.setAttribute("content", resolved === "dark" ? "#000000" : "#ffffff");
  }
  return resolved;
}

export function toggleThemeWithTransition(
  originEl: HTMLElement | null,
  reduceMotion: boolean,
  applyUpdate: () => void,
): void {
  if (typeof document === "undefined") {
    applyUpdate();
    return;
  }

  const root = document.documentElement;
  const prefersReduced =
    reduceMotion ||
    (typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches);

  if (prefersReduced) {
    applyUpdate();
    return;
  }

  if (originEl) {
    const rect = originEl.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const radius = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y));
    root.style.setProperty("--soxu-vt-x", `${x}px`);
    root.style.setProperty("--soxu-vt-y", `${y}px`);
    root.style.setProperty("--soxu-vt-r", `${radius}px`);
  }

  const docAny = document as Document & {
    startViewTransition?: (cb: () => void) => { finished: Promise<void> };
  };

  if (typeof docAny.startViewTransition === "function") {
    try {
      docAny.startViewTransition(() => {
        applyUpdate();
      });
      return;
    } catch {
      // 回退至全局色彩过渡
    }
  }

  root.classList.add("soxu-theme-anim");
  applyUpdate();
  window.setTimeout(() => {
    root.classList.remove("soxu-theme-anim");
  }, 240);
}
