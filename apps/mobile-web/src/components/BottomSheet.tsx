import type { ComponentChildren } from "preact";
import { useEffect } from "preact/hooks";
import { IconClose } from "../icons.tsx";

interface BottomSheetProps {
  title: string;
  subtitle?: string;
  tall?: boolean;
  onClose: () => void;
  children: ComponentChildren;
}

export function BottomSheet({ title, subtitle, tall = false, onClose, children }: BottomSheetProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      class="fixed inset-0 flex items-end justify-center"
      style={{
        backgroundColor: "var(--soxu-surface-scrim)",
        zIndex: "var(--soxu-z-drawer)",
      }}
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div
        class="w-full max-w-[680px] rounded-t-[22px] flex flex-col soxu-safe-bottom"
        style={{
          maxHeight: tall ? "76dvh" : "56dvh",
          backgroundColor: "var(--soxu-surface-card)",
          borderTop: "1px solid var(--soxu-border-strong)",
          borderLeft: "1px solid var(--soxu-border-subtle)",
          borderRight: "1px solid var(--soxu-border-subtle)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div class="pt-2.5 pb-1 flex justify-center">
          <div class="w-9 h-1 rounded-full" style={{ backgroundColor: "var(--soxu-border-strong)" }} />
        </div>
        <div
          class="px-4 py-2.5 flex items-center justify-between gap-3"
          style={{ borderBottom: "1px solid var(--soxu-border-hairline)" }}
        >
          <div class="min-w-0">
            <h3 class="m-0 text-sm font-semibold truncate">{title}</h3>
            {subtitle && (
              <p class="m-0 text-xs truncate" style={{ color: "var(--soxu-text-tertiary)" }}>
                {subtitle}
              </p>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            class="soxu-touch rounded-full flex items-center justify-center shrink-0"
            style={{
              backgroundColor: "var(--soxu-surface-control)",
              color: "var(--soxu-text-primary)",
            }}
            aria-label="关闭抽屉"
          >
            <IconClose size={16} />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-2.5">{children}</div>
      </div>
    </div>
  );
}
