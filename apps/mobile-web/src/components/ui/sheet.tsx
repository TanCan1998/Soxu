import { X } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import type * as React from "react";
import { useRef, useState } from "react";
import { cn } from "@/lib/utils";

export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export function SheetContent({
  className,
  children,
  side = "bottom",
  tall = false,
  rawBody = false,
  srTitle,
  title,
  subtitle,
  onRequestClose,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  side?: "bottom" | "right" | "left";
  tall?: boolean;
  rawBody?: boolean;
  srTitle?: string;
  title?: string;
  subtitle?: string;
  onRequestClose?: () => void;
}) {
  const [dragY, setDragY] = useState(0);
  const startYRef = useRef<number | null>(null);
  const closeBtnRef = useRef<HTMLButtonElement>(null);

  const posClasses =
    side === "bottom"
      ? cn(
          "inset-x-0 bottom-0 mx-auto w-full max-w-[680px] rounded-t-[22px] border-t border-x border-[var(--soxu-border-strong)] soxu-safe-bottom soxu-safe-x",
          "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-bottom data-[state=closed]:slide-out-to-bottom duration-200",
          tall
            ? "max-h-[min(76dvh,calc(100dvh-20px))]"
            : "max-h-[min(58dvh,calc(100dvh-20px))]",
        )
      : side === "right"
        ? "inset-y-0 right-0 h-full w-[min(380px,92vw)] border-l border-[var(--soxu-border-strong)] soxu-safe-right data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-right data-[state=closed]:slide-out-to-right duration-200"
        : "inset-y-0 left-0 h-full w-[min(300px,86vw)] border-r border-[var(--soxu-border-strong)] soxu-safe-left data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-left data-[state=closed]:slide-out-to-left duration-200";

  function handleHandlePointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (side !== "bottom") return;
    startYRef.current = e.clientY;
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function handleHandlePointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (startYRef.current === null) return;
    const delta = Math.max(0, e.clientY - startYRef.current);
    setDragY(delta);
  }

  function handleHandlePointerUp(e: React.PointerEvent<HTMLDivElement>) {
    if (startYRef.current === null) return;
    const delta = Math.max(0, e.clientY - startYRef.current);
    startYRef.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    setDragY(0);
    if (delta > 72) {
      if (onRequestClose) {
        onRequestClose();
      } else {
        closeBtnRef.current?.click();
      }
    }
  }

  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-[var(--soxu-z-drawer-scrim)] bg-[var(--soxu-surface-scrim)] backdrop-blur-[2px] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0 duration-150" />
      <DialogPrimitive.Content
        style={side === "bottom" && dragY > 0 ? { transform: `translateY(${dragY}px)` } : undefined}
        className={cn(
          "fixed z-[var(--soxu-z-drawer)] flex flex-col bg-[var(--soxu-surface-card)] text-[var(--soxu-text-primary)] outline-none",
          dragY === 0 && "transition-transform duration-150",
          posClasses,
          className,
        )}
        {...props}
      >
        {!title && (
          <>
            <DialogPrimitive.Title className="sr-only">{srTitle ?? "侧边抽屉面板"}</DialogPrimitive.Title>
            <DialogPrimitive.Description className="sr-only">{srTitle ?? "侧边滑动面板"}</DialogPrimitive.Description>
          </>
        )}
        {side === "bottom" && (
          <div
            onPointerDown={handleHandlePointerDown}
            onPointerMove={handleHandlePointerMove}
            onPointerUp={handleHandlePointerUp}
            onPointerCancel={() => {
              startYRef.current = null;
              setDragY(0);
            }}
            className="pt-2.5 pb-1.5 flex justify-center shrink-0 touch-none cursor-grab active:cursor-grabbing"
          >
            <div className="w-9 h-1 rounded-full bg-[var(--soxu-border-strong)]" />
          </div>
        )}
        {title && (
          <div className="px-4 py-2.5 flex items-center justify-between gap-3 border-b border-[var(--soxu-border-hairline)] shrink-0">
            <div className="min-w-0">
              <DialogPrimitive.Title className="text-sm font-semibold truncate">{title}</DialogPrimitive.Title>
              {subtitle && (
                <DialogPrimitive.Description className="text-xs text-[var(--soxu-text-tertiary)] truncate">
                  {subtitle}
                </DialogPrimitive.Description>
              )}
            </div>
            <DialogPrimitive.Close
              ref={closeBtnRef}
              className="soxu-touch inline-flex items-center justify-center rounded-full bg-[var(--soxu-surface-control)] text-[var(--soxu-text-primary)] shrink-0 cursor-pointer hover:bg-[var(--soxu-interaction-selected)]"
              aria-label="关闭抽屉"
            >
              <X className="h-4 w-4" strokeWidth={1.75} />
            </DialogPrimitive.Close>
          </div>
        )}
        {!title && side === "bottom" && (
          <DialogPrimitive.Close ref={closeBtnRef} className="sr-only" aria-label="关闭抽屉" />
        )}
        <div
          className={cn(
            "flex-1 min-h-0 flex flex-col",
            rawBody ? "overflow-hidden" : "overflow-y-auto px-4 py-3 gap-2.5 soxu-scroll",
          )}
        >
          {children}
        </div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
