import type * as React from "react";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

interface SinkLayoutProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  reduceMotion?: boolean;
  sidebar: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
  triggerRef?: React.RefObject<HTMLElement | null>;
}

/**
 * 移动端下沉式侧栏布局（吸收 Lynricsy/Oryn SinkLayout 设计）：
 * - 侧栏常驻底层（全屏高、#0D0D0D 底色、顶部安全区内边距），底部的全局胶囊导航栏置于 footer 槽中，
 *   平时被上层主对话卡片完全遮盖，只在拉出侧栏时完整浮现；
 * - 顶层主对话卡片打开时平滑右移 min(84vw, 316px)，呈现圆角与细高光描边；
 * - 支持左缘向右滑拉出与在卡片右缘条左滑/轻点关闭，关闭时立刻释放 inert 并还原焦点。
 */
export function SinkLayout({
  open,
  onOpenChange,
  reduceMotion = false,
  sidebar,
  footer,
  children,
  triggerRef,
}: SinkLayoutProps) {
  const [dragOffset, setDragOffset] = useState<number | null>(null);
  const startRef = useRef<{ x: number; y: number; time: number; mode: "open" | "close" } | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const sidebarWidth =
    typeof window !== "undefined" ? Math.min(Math.round(window.innerWidth * 0.84), 316) : 300;

  useEffect(() => {
    const el = cardRef.current as (HTMLDivElement & { inert?: boolean }) | null;
    if (el) {
      el.inert = open;
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onOpenChange(false);
        window.setTimeout(() => triggerRef?.current?.focus(), 20);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onOpenChange, triggerRef]);

  function handlePointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if (!open && e.clientX <= 24) {
      startRef.current = { x: e.clientX, y: e.clientY, time: performance.now(), mode: "open" };
    } else if (open) {
      startRef.current = { x: e.clientX, y: e.clientY, time: performance.now(), mode: "close" };
    }
  }

  function handlePointerMove(e: React.PointerEvent<HTMLDivElement>) {
    const st = startRef.current;
    if (!st) return;
    const dx = e.clientX - st.x;
    const dy = Math.abs(e.clientY - st.y);
    if (dy > 28 && dragOffset === null) {
      startRef.current = null;
      return;
    }
    if (st.mode === "open" && dx > 6) {
      setDragOffset(Math.min(sidebarWidth, Math.max(0, dx)));
    } else if (st.mode === "close" && dx < -6) {
      setDragOffset(Math.max(0, Math.min(sidebarWidth, sidebarWidth + dx)));
    }
  }

  function handlePointerUp(e: React.PointerEvent<HTMLDivElement>) {
    const st = startRef.current;
    startRef.current = null;
    if (!st || dragOffset === null) {
      setDragOffset(null);
      return;
    }
    const dt = Math.max(1, performance.now() - st.time);
    const vx = (e.clientX - st.x) / dt;
    if (st.mode === "open") {
      onOpenChange(vx > 0.3 || dragOffset > sidebarWidth * 0.36);
    } else {
      onOpenChange(!(vx < -0.3 || dragOffset < sidebarWidth * 0.64));
    }
    setDragOffset(null);
  }

  const translateX = dragOffset !== null ? dragOffset : open ? sidebarWidth : 0;
  const isCardLifted = translateX > 2;

  return (
    <div
      className="relative h-dvh w-full overflow-hidden bg-[var(--soxu-surface-sidebar)]"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={() => {
        startRef.current = null;
        setDragOffset(null);
      }}
    >
      {/* 底层侧栏层 + 仅打开时露出的底部切换胶囊 */}
      <div
        aria-hidden={!open}
        style={{ width: `${sidebarWidth}px` }}
        className={cn(
          "absolute inset-y-0 left-0 flex flex-col bg-[var(--soxu-surface-sidebar)] soxu-safe-top soxu-safe-left",
          !open && "pointer-events-none",
        )}
      >
        <div className="flex-1 min-h-0 overflow-y-auto soxu-scroll">{sidebar}</div>
        {footer && (
          <div className="shrink-0 px-3 pt-2 pb-[max(12px,env(safe-area-inset-bottom))] bg-[var(--soxu-surface-sidebar)] border-t border-[var(--soxu-border-hairline)]">
            {footer}
          </div>
        )}
      </div>

      {/* 顶层主视图卡片 */}
      <div
        ref={cardRef}
        style={{
          transform: translateX > 0 ? `translate3d(${translateX}px, 0, 0)` : undefined,
        }}
        className={cn(
          "relative z-10 h-full w-full flex flex-col bg-[var(--soxu-surface-canvas)] overflow-hidden",
          dragOffset === null && !reduceMotion && "transition-[transform,border-radius] duration-[260ms] ease-[var(--soxu-ease-out)]",
          isCardLifted
            ? "rounded-[22px] border border-[var(--soxu-border-strong)] shadow-[0_12px_40px_rgba(0,0,0,0.85)]"
            : "rounded-none border-0",
        )}
      >
        {children}
      </div>

      {/* 侧栏打开时：右侧露出的卡片点击/轻滑关闭热区 */}
      {open && (
        <button
          type="button"
          onClick={() => {
            onOpenChange(false);
            window.setTimeout(() => triggerRef?.current?.focus(), 20);
          }}
          aria-label="收起侧栏返回对话"
          style={{ left: `${sidebarWidth}px` }}
          className="fixed inset-y-0 right-0 z-20 bg-transparent border-0 cursor-pointer"
        />
      )}
    </div>
  );
}
