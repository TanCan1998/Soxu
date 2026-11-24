import { X } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export function SheetContent({
  className,
  children,
  side = "bottom",
  tall = false,
  title,
  subtitle,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  side?: "bottom" | "right" | "left";
  tall?: boolean;
  title?: string;
  subtitle?: string;
}) {
  const posClasses =
    side === "bottom"
      ? cn(
          "inset-x-0 bottom-0 mx-auto w-full max-w-[680px] rounded-t-[22px] border-t border-x border-[var(--soxu-border-strong)] soxu-safe-bottom",
          "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-bottom data-[state=closed]:slide-out-to-bottom duration-200",
          tall ? "max-h-[76dvh]" : "max-h-[58dvh]",
        )
      : side === "right"
        ? "inset-y-0 right-0 h-full w-[min(360px,92vw)] border-l border-[var(--soxu-border-strong)] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-right data-[state=closed]:slide-out-to-right duration-200"
        : "inset-y-0 left-0 h-full w-[min(320px,88vw)] border-r border-[var(--soxu-border-strong)] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:slide-in-from-left data-[state=closed]:slide-out-to-left duration-200";

  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-[var(--soxu-z-modal-scrim)] bg-[var(--soxu-surface-scrim)] backdrop-blur-[2px] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0 duration-150" />
      <DialogPrimitive.Content
        className={cn(
          "fixed z-[var(--soxu-z-drawer)] flex flex-col bg-[var(--soxu-surface-card)] text-[var(--soxu-text-primary)] outline-none",
          posClasses,
          className,
        )}
        {...props}
      >
        {side === "bottom" && (
          <div className="pt-2.5 pb-1 flex justify-center shrink-0">
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
              className="soxu-touch inline-flex items-center justify-center rounded-full bg-[var(--soxu-surface-control)] text-[var(--soxu-text-primary)] shrink-0 cursor-pointer hover:bg-[var(--soxu-interaction-selected)]"
              aria-label="关闭抽屉"
            >
              <X className="h-4 w-4" strokeWidth={1.75} />
            </DialogPrimitive.Close>
          </div>
        )}
        <div className="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-2.5 soxu-scroll">{children}</div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
