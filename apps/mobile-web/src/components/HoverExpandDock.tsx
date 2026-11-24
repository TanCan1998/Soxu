import type * as React from "react";
import { useEffect, useRef, useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface HoverExpandDockProps {
  triggerIcon: React.ReactNode;
  triggerLabel: string;
  dotColor?: string;
  locked?: boolean;
  expandDirection?: "left" | "right";
  children: React.ReactNode;
  className?: string;
}

/**
 * 极简悬停自动展开收纳容器：
 * - 常态仅展示单一极简图标（可带 6px 状态点）；
 * - 鼠标划过 40ms 或键盘 focus-within 时平滑展开子工具栏；
 * - 移开 160ms 防抖收起；若子浮层（Popover/Dropdown/Modal）打开则锁定展开态。
 */
export function HoverExpandDock({
  triggerIcon,
  triggerLabel,
  dotColor,
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

  function handleMouseEnter() {
    clearTimers();
    openTimer.current = window.setTimeout(() => setHovered(true), 40);
  }

  function handleMouseLeave() {
    clearTimers();
    closeTimer.current = window.setTimeout(() => setHovered(false), 160);
  }

  useEffect(() => {
    if (!pinned) return;
    const onOutside = (ev: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(ev.target as Node)) {
        setPinned(false);
      }
    };
    document.addEventListener("mousedown", onOutside);
    return () => document.removeEventListener("mousedown", onOutside);
  }, [pinned]);

  useEffect(() => () => clearTimers(), []);

  return (
    <div
      ref={containerRef}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onFocus={() => setHovered(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setHovered(false);
        }
      }}
      className={cn(
        "group/dock inline-flex items-center rounded-full border transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
        isExpanded
          ? "border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-1 py-0.5 gap-0.5"
          : "border-transparent bg-transparent p-0",
        expandDirection === "right" ? "flex-row" : "flex-row-reverse",
        className,
      )}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={() => setPinned((p) => !p)}
            aria-expanded={isExpanded}
            aria-label={triggerLabel}
            className={cn(
              "relative inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] transition-colors cursor-pointer",
              pinned && "bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)]",
            )}
          >
            {triggerIcon}
            {dotColor && (
              <span
                className="absolute right-1 top-1 h-1.5 w-1.5 rounded-full"
                style={{ backgroundColor: dotColor }}
                aria-hidden="true"
              />
            )}
          </button>
        </TooltipTrigger>
        <TooltipContent>{triggerLabel}</TooltipContent>
      </Tooltip>

      <div
        className={cn(
          "inline-flex items-center gap-0.5 overflow-hidden transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
          isExpanded ? "max-w-[420px] opacity-100 translate-x-0" : "max-w-0 opacity-0 pointer-events-none",
          !isExpanded && (expandDirection === "right" ? "-translate-x-1.5" : "translate-x-1.5"),
        )}
      >
        {children}
      </div>
    </div>
  );
}
