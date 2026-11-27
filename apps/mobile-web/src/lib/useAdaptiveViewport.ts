import type * as React from "react";
import { useEffect, useMemo, useState } from "react";
import type { FoldPreviewMode, SoxuPreferences, WindowChromeStyle } from "@/lib/preferences";
import type { SizeClass } from "@/types";

export type FoldPosture = "none" | "dual-horizontal" | "dual-vertical";

export interface AdaptiveViewportState {
  width: number;
  height: number;
  sizeClass: SizeClass;
  isUltraNarrow: boolean;
  isShortViewport: boolean;
  isCoarsePointer: boolean;
  canHover: boolean;
  foldPosture: FoldPosture;
  leftSegmentWidth: number;
  rightSegmentWidth: number;
  hingeWidth: number;
  topSegmentHeight: number;
  bottomSegmentHeight: number;
  hingeHeight: number;
  keyboardInset: number;
  keyboardOpen: boolean;
  isMac: boolean;
}

/**
 * 判断键盘事件是否处于中文/日文/韩文等输入法组合选词阶段，
 * 避免按 Enter 上屏选词时误触发发送或提交。
 */
export function isImeComposing(e: React.KeyboardEvent | KeyboardEvent): boolean {
  const native = "nativeEvent" in e ? e.nativeEvent : e;
  return Boolean(native.isComposing || (native as KeyboardEvent).keyCode === 229);
}

/**
 * 按当前操作系统或用户预览设置格式化跨平台快捷键。
 * 严格避免使用可能触发 Emoji 规范告警的符号，保留标准 macOS 技术符号与清晰英文键位。
 */
export function formatShortcut(spec: string, isMac: boolean): string {
  if (!isMac) {
    return spec.replaceAll("Mod+", "Ctrl+");
  }
  return spec
    .replaceAll("Mod+Shift+", "\u2318\u21E7")
    .replaceAll("Mod+", "\u2318")
    .replaceAll("Alt+", "\u2325")
    .replaceAll("Enter", "Return");
}

export function detectMacPlatform(override?: WindowChromeStyle): boolean {
  if (override === "macos") return true;
  if (override === "windows") return false;
  if (typeof navigator === "undefined") return false;
  const uaData = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData;
  const platform = (uaData?.platform ?? navigator.platform ?? navigator.userAgent ?? "").toLowerCase();
  return platform.includes("mac") || platform.includes("darwin");
}

function resolveSizeClass(width: number): SizeClass {
  if (width >= 1024) return "wide";
  if (width >= 640) return "regular";
  return "compact";
}

function readViewportSegments(): DOMRect[] | null {
  if (typeof window === "undefined") return null;
  const win = window as Window & { viewport?: { segments?: DOMRect[] } };
  const segs = win.viewport?.segments;
  if (!Array.isArray(segs) || segs.length < 2) return null;
  return segs;
}

function readUrlFoldOverride(): FoldPreviewMode | null {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  const fold = params.get("fold");
  if (fold === "dual-h") return "dual-horizontal";
  if (fold === "dual-v") return "dual-vertical";
  return null;
}

function computeAdaptiveSnapshot(
  prefs?: Pick<SoxuPreferences, "windowChromeStyle" | "foldPreviewMode">,
): AdaptiveViewportState {
  const width = typeof window !== "undefined" ? window.innerWidth : 1280;
  const height = typeof window !== "undefined" ? window.innerHeight : 800;
  const isCoarsePointer =
    typeof window !== "undefined" ? window.matchMedia("(pointer: coarse)").matches : false;
  const canHover =
    typeof window !== "undefined"
      ? window.matchMedia("(hover: hover) and (pointer: fine)").matches
      : true;

  let keyboardInset = 0;
  if (typeof window !== "undefined" && window.visualViewport) {
    const vv = window.visualViewport;
    const rawDiff = Math.round(window.innerHeight - vv.height - vv.offsetTop);
    if (rawDiff > 120) {
      keyboardInset = rawDiff;
    }
  }

  let foldPosture: FoldPosture = "none";
  let leftSegmentWidth = Math.floor(width / 2);
  let rightSegmentWidth = Math.floor(width / 2);
  let hingeWidth = 0;
  let topSegmentHeight = Math.floor(height / 2);
  let bottomSegmentHeight = Math.floor(height / 2);
  let hingeHeight = 0;

  const segs = readViewportSegments();
  if (segs && segs.length >= 2) {
    const a = segs[0];
    const b = segs[1];
    if (b.left > a.right - 2 && b.width > 0) {
      foldPosture = "dual-horizontal";
      leftSegmentWidth = Math.round(a.width);
      rightSegmentWidth = Math.round(b.width);
      hingeWidth = Math.max(0, Math.round(b.left - a.right));
    } else if (b.top > a.bottom - 2 && b.height > 0) {
      foldPosture = "dual-vertical";
      topSegmentHeight = Math.round(a.height);
      bottomSegmentHeight = Math.round(b.height);
      hingeHeight = Math.max(0, Math.round(b.top - a.bottom));
    }
  } else {
    const previewMode = readUrlFoldOverride() ?? prefs?.foldPreviewMode ?? "auto";
    if (previewMode === "dual-horizontal" && width >= 600) {
      foldPosture = "dual-horizontal";
      hingeWidth = 28;
      leftSegmentWidth = Math.floor((width - hingeWidth) / 2);
      rightSegmentWidth = width - hingeWidth - leftSegmentWidth;
    } else if (previewMode === "dual-vertical" && height >= 420) {
      foldPosture = "dual-vertical";
      hingeHeight = 24;
      topSegmentHeight = Math.round((height - hingeHeight) * 0.54);
      bottomSegmentHeight = Math.max(150, height - hingeHeight - topSegmentHeight);
    }
  }

  return {
    width,
    height,
    sizeClass: resolveSizeClass(width),
    isUltraNarrow: width < 400,
    isShortViewport: height <= 500,
    isCoarsePointer,
    canHover,
    foldPosture,
    leftSegmentWidth,
    rightSegmentWidth,
    hingeWidth,
    topSegmentHeight,
    bottomSegmentHeight,
    hingeHeight,
    keyboardInset,
    keyboardOpen: keyboardInset > 120,
    isMac: detectMacPlatform(prefs?.windowChromeStyle),
  };
}

/**
 * 统一跨形态屏幕、折叠屏分段、软键盘贴合与输入指针能力 Hook。
 * 严格遵循「只响应窗口几何、分段与指针能力，绝不按 UA 猜设备型号」原则。
 */
export function useAdaptiveViewport(
  prefs?: Pick<SoxuPreferences, "windowChromeStyle" | "foldPreviewMode">,
): AdaptiveViewportState {
  const [snap, setSnap] = useState<AdaptiveViewportState>(() => computeAdaptiveSnapshot(prefs));

  useEffect(() => {
    const update = () => {
      const next = computeAdaptiveSnapshot(prefs);
      setSnap(next);
      if (typeof document !== "undefined") {
        document.documentElement.style.setProperty("--soxu-kb-inset", `${next.keyboardInset}px`);
      }
    };

    update();
    window.addEventListener("resize", update);
    const vv = window.visualViewport;
    if (vv) {
      vv.addEventListener("resize", update);
      vv.addEventListener("scroll", update);
    }
    const coarseMq = window.matchMedia("(pointer: coarse)");
    const hoverMq = window.matchMedia("(hover: hover) and (pointer: fine)");
    coarseMq.addEventListener("change", update);
    hoverMq.addEventListener("change", update);

    return () => {
      window.removeEventListener("resize", update);
      if (vv) {
        vv.removeEventListener("resize", update);
        vv.removeEventListener("scroll", update);
      }
      coarseMq.removeEventListener("change", update);
      hoverMq.removeEventListener("change", update);
    };
  }, [prefs?.windowChromeStyle, prefs?.foldPreviewMode]);

  return useMemo(() => snap, [snap]);
}
