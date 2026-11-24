export type ThemeMode = "dark" | "light" | "system";
export type DensityMode = "compact" | "comfortable";
export type UiFontPresetId = "inter-noto" | "ibm-plex-noto" | "jakarta-noto" | "lxgw-wenkai" | "system-sans";
export type CodeFontPresetId = "jetbrains-mono" | "fira-code" | "maple-mono" | "ibm-plex-mono" | "cascadia-code";

export interface FontPreset<T extends string> {
  id: T;
  name: string;
  license: string;
  stack: string;
}

export const UI_FONT_PRESETS: FontPreset<UiFontPresetId>[] = [
  {
    id: "inter-noto",
    name: "Inter + 思源黑体 (Noto Sans SC)",
    license: "SIL OFL 1.1",
    stack: "'Inter', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "ibm-plex-noto",
    name: "IBM Plex Sans + 思源黑体",
    license: "SIL OFL 1.1",
    stack: "'IBM Plex Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "jakarta-noto",
    name: "Plus Jakarta Sans + 思源黑体",
    license: "SIL OFL 1.1",
    stack: "'Plus Jakarta Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif",
  },
  {
    id: "lxgw-wenkai",
    name: "霞鹜文楷 (LXGW WenKai) + Inter",
    license: "SIL OFL 1.1",
    stack: "'Inter', 'LXGW WenKai Screen', 'LXGW WenKai', 'Noto Sans SC', sans-serif",
  },
  {
    id: "system-sans",
    name: "系统原生无衬线回退",
    license: "OS Default",
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
    name: "Maple Mono (中英 2:1 对齐)",
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
  uiFontPreset: UiFontPresetId;
  codeFollowsUi: boolean;
  codeFontPreset: CodeFontPresetId;
  uiFontSize: number;
  codeFontSize: number;
  codeLigatures: boolean;
  reduceMotion: boolean;
  autoCheckpoint: boolean;
  defaultBranch: string;
}

export const DEFAULT_PREFERENCES: SoxuPreferences = {
  themeMode: "dark",
  density: "compact",
  uiFontPreset: "inter-noto",
  codeFollowsUi: false,
  codeFontPreset: "jetbrains-mono",
  uiFontSize: 14,
  codeFontSize: 13,
  codeLigatures: true,
  reduceMotion: false,
  autoCheckpoint: true,
  defaultBranch: "origin/main",
};

const STORAGE_KEY = "soxu.preferences.v1";

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
    // 静态 preview 或隐私模式下忽略存储异常
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
      // 回退至色彩插值过渡
    }
  }

  root.classList.add("soxu-theme-anim");
  applyUpdate();
  window.setTimeout(() => {
    root.classList.remove("soxu-theme-anim");
  }, 240);
}
