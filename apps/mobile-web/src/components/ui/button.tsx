import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-xs font-medium transition-all duration-150 ease-[var(--soxu-ease-out)] cursor-pointer disabled:pointer-events-none disabled:opacity-45 active:scale-[0.98] [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-[var(--soxu-action-inverse)] text-[var(--soxu-action-on-inverse)] font-semibold hover:bg-[var(--soxu-action-inverse-hover)]",
        secondary:
          "bg-[var(--soxu-surface-control)] text-[var(--soxu-text-primary)] border border-[var(--soxu-border-subtle)] hover:bg-[var(--soxu-interaction-selected)] hover:border-[var(--soxu-border-strong)]",
        outline:
          "border border-[var(--soxu-border-subtle)] bg-transparent text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)]",
        ghost:
          "bg-transparent text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
        destructive:
          "bg-[var(--soxu-semantic-danger-soft)] text-[var(--soxu-semantic-danger)] border border-[var(--soxu-semantic-danger-border)] hover:brightness-110",
      },
      size: {
        default: "h-8 px-3",
        sm: "h-7 px-2.5 text-[11.5px]",
        lg: "h-11 px-4 text-sm rounded-xl",
        icon: "h-8 w-8 rounded-full",
        "icon-sm": "h-7 w-7 rounded-md",
      },
    },
    defaultVariants: {
      variant: "secondary",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export function Button({ className, variant, size, type = "button", ...props }: ButtonProps) {
  return <button type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}

export { buttonVariants };
