import { Tabs as TabsPrimitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

export const Tabs = TabsPrimitive.Root;

export function TabsList({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.List>) {
  return (
    <TabsPrimitive.List
      className={cn(
        "inline-flex items-center gap-0.5 rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]",
        className,
      )}
      {...props}
    />
  );
}

export function TabsTrigger({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium text-[var(--soxu-text-tertiary)] transition-all duration-150 cursor-pointer hover:text-[var(--soxu-text-secondary)] data-[state=active]:bg-[var(--soxu-surface-raised)] data-[state=active]:text-[var(--soxu-text-primary)] data-[state=active]:border data-[state=active]:border-[var(--soxu-border-subtle)]",
        className,
      )}
      {...props}
    />
  );
}

export function TabsContent({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Content>) {
  return <TabsPrimitive.Content className={cn("outline-none animate-in fade-in-0 duration-150", className)} {...props} />;
}
