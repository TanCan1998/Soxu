import type * as React from "react";
import { useEffect, useRef, useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface HoverExpandDockProps {
  triggerIcon: React.ReactNode;
  triggerLabel: string;
  locked?: boolean;
  expandDirection?: "left" | "right";
  children: React.ReactNode;
  className?: string;
}

/**
 * 极简悬停自动展开收纳容器：
 * - 常态仅展示单一纯净线性图标，不附加角标；
 * - 鼠标指针划过 40ms 或键盘聚焦时平滑展开子工具栏；
 * - 触控与触控笔点击触发键直接锁定展开，外部 pointerdown 或 Esc 收起。
 */
export function HoverExpandDock({
  triggerIcon,
  triggerLabel,
  locked = false,
  expandDirection = "left",
  children,
  className,
}: HoverExpandDockProps) {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const openTimer = useRef<number | null>(null);
  const closeTimer = useRef<number | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const isExpanded = hovered || pinned || locked;

  function clearTimers() {
    if (openTimer.current !== null) {
      window.clearTimeout(openTimer.current);
      openTimer.current = null;
    }
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  }

  function handlePointerEnter(e: React.PointerEvent<HTMLDivElement>) {
    if (e.pointerType !== "mouse") return;
    clearTimers();
    openTimer.current = window.setTimeout(() => setHovered(true), 40);
  }

  function handlePointerLeave(e: React.PointerEvent<HTMLDivElement>) {
    if (e.pointerType !== "mouse") return;
    clearTimers();
    closeTimer.current = window.setTimeout(() => setHovered(false), 160);
  }

  useEffect(() => {
    if (!pinned && !hovered) return;
    const onOutside = (ev: PointerEvent) => {
      if (containerRef.current && !containerRef.current.contains(ev.target as Node)) {
        setPinned(false);
        setHovered(false);
      }
    };
    const onKeyDown = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        setPinned(false);
        setHovered(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("pointerdown", onOutside);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onOutside);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [pinned, hovered]);

  useEffect(() => () => clearTimers(), []);

  return (
    <div
      ref={containerRef}
      onPointerEnter={handlePointerEnter}
      onPointerLeave={handlePointerLeave}
      onFocus={() => setHovered(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setHovered(false);
        }
      }}
      className={cn(
        "group/dock relative inline-flex items-center rounded-full border transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)] shrink-0",
        isExpanded
          ? "lg:border-[var(--soxu-border-subtle)] lg:bg-[var(--soxu-surface-card)] lg:px-1 lg:py-0.5 lg:gap-0.5 border-transparent bg-transparent p-0"
          : "border-transparent bg-transparent p-0",
        expandDirection === "right" ? "flex-row" : "flex-row-reverse",
        className,
      )}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            ref={triggerRef}
            type="button"
            onClick={() => setPinned((p) => !p)}
            aria-expanded={isExpanded}
            aria-label={triggerLabel}
            className={cn(
              "soxu-hit-44 relative inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] transition-colors cursor-pointer",
              pinned && "bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)]",
            )}
          >
            {triggerIcon}
          </button>
        </TooltipTrigger>
        <TooltipContent>{triggerLabel}</TooltipContent>
      </Tooltip>

      <div
        className={cn(
          "inline-flex items-center gap-1 overflow-hidden transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
          "max-lg:absolute max-lg:top-[calc(100%+6px)] max-lg:z-[var(--soxu-z-popover)] max-lg:rounded-full max-lg:border max-lg:border-[var(--soxu-border-strong)] max-lg:bg-[var(--soxu-surface-floating)] max-lg:px-1.5 max-lg:py-1",
          expandDirection === "right" ? "max-lg:left-0" : "max-lg:right-0",
          isExpanded
            ? "max-lg:max-w-[calc(100vw-24px)] lg:max-w-[420px] opacity-100 translate-x-0"
            : "max-w-0 opacity-0 pointer-events-none max-lg:p-0 max-lg:border-0",
          !isExpanded && (expandDirection === "right" ? "-translate-x-1.5" : "translate-x-1.5"),
        )}
      >
        {children}
      </div>
    </div>
  );
}
