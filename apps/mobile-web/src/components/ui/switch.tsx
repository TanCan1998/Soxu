import { Switch as SwitchPrimitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

export function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      className={cn(
        "peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-control)] transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-45 data-[state=checked]:bg-[var(--soxu-action-inverse)] data-[state=checked]:border-transparent",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="pointer-events-none block h-3.5 w-3.5 rounded-full bg-[var(--soxu-text-secondary)] transition-transform duration-150 ease-[var(--soxu-ease-out)] translate-x-0.5 data-[state=checked]:translate-x-[18px] data-[state=checked]:bg-[var(--soxu-action-on-inverse)]" />
    </SwitchPrimitive.Root>
  );
}
