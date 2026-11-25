import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-tight whitespace-nowrap transition-colors duration-150",
  {
    variants: {
      variant: {
        neutral: "bg-[var(--soxu-surface-control)] text-[var(--soxu-text-secondary)] border border-[var(--soxu-border-subtle)]",
        accent: "bg-[var(--soxu-semantic-accent-soft)] text-[var(--soxu-semantic-accent)]",
        running: "bg-[var(--soxu-semantic-running-soft)] text-[var(--soxu-semantic-running)]",
        warning: "bg-[var(--soxu-semantic-warning-soft)] text-[var(--soxu-semantic-warning)]",
        danger: "bg-[var(--soxu-semantic-danger-soft)] text-[var(--soxu-semantic-danger)]",
        handoff: "bg-[var(--soxu-semantic-handoff-soft)] text-[var(--soxu-semantic-handoff)]",
      },
    },
    defaultVariants: {
      variant: "neutral",
    },
  },
);

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
