import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface ThemeToggleProps {
  resolvedTheme: "dark" | "light";
  onToggle: (originEl: HTMLElement | null) => void;
  className?: string;
}

/**
 * 纯图标主题切换按钮（仅 Sun / Moon 矢量图标，无文本）。
 * 对应 GPUI `ThemeSnapshot::lerp` 与 SVG 旋转变换动画。
 */
export function ThemeToggle({ resolvedTheme, onToggle, className }: ThemeToggleProps) {
  const isDark = resolvedTheme === "dark";
  const label = isDark ? "切换至日光浅色主题" : "切换至 Pure Black 纯黑主题";

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={(e) => onToggle(e.currentTarget)}
          className={cn(
            "relative overflow-hidden rounded-full border border-transparent hover:border-[var(--soxu-border-subtle)]",
            className,
          )}
          aria-label={label}
        >
          <Sun
            strokeWidth={1.75}
            className={cn(
              "h-4 w-4 transition-all duration-[var(--soxu-motion-theme-icon)] ease-[var(--soxu-ease-out)]",
              isDark ? "rotate-90 scale-50 opacity-0" : "rotate-0 scale-100 opacity-100",
            )}
          />
          <Moon
            strokeWidth={1.75}
            className={cn(
              "absolute h-4 w-4 transition-all duration-[var(--soxu-motion-theme-icon)] ease-[var(--soxu-ease-out)]",
              isDark ? "rotate-0 scale-100 opacity-100" : "-rotate-90 scale-50 opacity-0",
            )}
          />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
