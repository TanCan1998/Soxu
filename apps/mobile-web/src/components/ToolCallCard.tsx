import { Check, ChevronRight, Cpu, FileCode2, Search, Terminal } from "lucide-react";
import { useState } from "react";
import { CodeDiffView, CodeSnippetView, type DiffRowItem } from "@/components/CodeBlockView";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import type { ToolCardItem } from "@/types";

interface ToolCallCardProps {
  tool: ToolCardItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  showLineNumbers: boolean;
  wrap: boolean;
  onSubmitLineComment?: (filePath: string, lineNum: number, codeLine: string, comment: string) => void;
}

function parseLinesToDiffRows(lines: string[]): DiffRowItem[] {
  let num = 22;
  return lines.map((line) => {
    if (line.startsWith("+")) {
      return { lineNum: num++, kind: "add" as const, code: line };
    }
    if (line.startsWith("-")) {
      return { kind: "del" as const, code: line };
    }
    return { lineNum: num++, kind: "ctx" as const, code: line };
  });
}

function kindMeta(kind: ToolCardItem["kind"]) {
  switch (kind) {
    case "read":
      return {
        label: "read",
        icon: <Search className="h-3.5 w-3.5" strokeWidth={1.75} />,
        boxClass: "bg-[var(--soxu-surface-control)] text-[var(--soxu-text-secondary)]",
      };
    case "edit":
      return {
        label: "edit",
        icon: <FileCode2 className="h-3.5 w-3.5" strokeWidth={1.75} />,
        boxClass: "bg-[var(--soxu-semantic-warning-soft)] text-[var(--soxu-semantic-warning)]",
      };
    case "bash":
      return {
        label: "bash",
        icon: <Terminal className="h-3.5 w-3.5" strokeWidth={1.75} />,
        boxClass: "bg-[var(--soxu-semantic-accent-soft)] text-[var(--soxu-semantic-accent)]",
      };
    case "mcp":
      return {
        label: "mcp",
        icon: <Cpu className="h-3.5 w-3.5" strokeWidth={1.75} />,
        boxClass: "bg-[var(--soxu-semantic-handoff-soft)] text-[var(--soxu-semantic-handoff)]",
      };
  }
}

/**
 * 统一结构化工具调用卡（吸收 Lynricsy/Oryn tool-cards-dark.png 设计）：
 * - 头部：22x22px 语义淡底小图标方块 + 工具名 + 意图描述 + 右侧核心读数(+N -M/耗时) + 状态圆勾 + 折叠箭头；
 * - 展开体不写冗余「输入/输出」标签，直接以表面层与 #050505 内嵌井区分；
 * - bash 卡支持在 [伪代码轨迹 | 原始命令] 之间原地切换。
 */
export function ToolCallCard({
  tool,
  open,
  onOpenChange,
  showLineNumbers,
  wrap,
  onSubmitLineComment,
}: ToolCallCardProps) {
  const [bashMode, setBashMode] = useState<"pseudo" | "raw">(tool.pseudoLines ? "pseudo" : "raw");
  const meta = kindMeta(tool.kind);

  return (
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] overflow-hidden"
    >
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="w-full min-h-[36px] px-3 py-1.5 bg-[var(--soxu-surface-card-muted)] hover:bg-[var(--soxu-interaction-hover)] flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
        >
          <span className="flex items-center gap-2 min-w-0">
            <span className={cn("h-[22px] w-[22px] rounded-md inline-flex items-center justify-center shrink-0", meta.boxClass)}>
              {meta.icon}
            </span>
            <span className="soxu-mono font-semibold text-[var(--soxu-text-primary)] shrink-0">{meta.label}</span>
            <span className="truncate text-[var(--soxu-text-secondary)]">{tool.intent ?? tool.title}</span>
          </span>

          <span className="flex items-center gap-2 shrink-0 text-[11px] soxu-mono soxu-tabular">
            {tool.kind === "edit" && (tool.diffAdd !== undefined || tool.diffDel !== undefined) ? (
              <span className="inline-flex items-center gap-1">
                <span className="text-[var(--soxu-semantic-success)]">+{tool.diffAdd ?? 0}</span>
                <span className="text-[var(--soxu-semantic-danger)]">-{tool.diffDel ?? 0}</span>
              </span>
            ) : (
              <span className="text-[var(--soxu-text-tertiary)] hidden sm:inline">{tool.metric ?? tool.status}</span>
            )}
            <span className="h-4 w-4 rounded-full border border-[var(--soxu-semantic-accent-border)] bg-[var(--soxu-semantic-accent-soft)] text-[var(--soxu-semantic-accent)] inline-flex items-center justify-center">
              <Check className="h-2.5 w-2.5" strokeWidth={2.2} />
            </span>
            <ChevronRight
              className={cn("h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] transition-transform duration-150", open && "rotate-90")}
              strokeWidth={1.75}
            />
          </span>
        </button>
      </CollapsibleTrigger>

      <CollapsibleContent className="soxu-collapsible overflow-hidden border-t border-[var(--soxu-border-hairline)] p-2.5 flex flex-col gap-2">
        {tool.kind === "bash" && tool.pseudoLines && (
          <div className="flex items-center justify-between gap-2 px-1">
            <span className="soxu-mono text-[11px] text-[var(--soxu-text-tertiary)] truncate">{tool.title}</span>
            <div className="inline-flex rounded-md bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)] shrink-0">
              {(
                [
                  { id: "pseudo", label: "轨迹" },
                  { id: "raw", label: "原始" },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setBashMode(tab.id)}
                  className={cn(
                    "px-2 py-0.5 rounded text-[10.5px] cursor-pointer transition-colors",
                    bashMode === tab.id
                      ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-medium"
                      : "text-[var(--soxu-text-tertiary)]",
                  )}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>
        )}

        {tool.kind === "edit" ? (
          <CodeDiffView
            rows={parseLinesToDiffRows(tool.lines)}
            layout="unified"
            showLineNumbers={showLineNumbers}
            wrap={wrap}
            filePath={tool.title.replace(/^修改\s+/, "")}
            onSubmitLineComment={onSubmitLineComment}
          />
        ) : tool.kind === "bash" && bashMode === "pseudo" && tool.pseudoLines ? (
          <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-2.5 soxu-code-block leading-relaxed flex flex-col gap-0.5 overflow-x-auto">
            {tool.pseudoLines.map((ln) => {
              const activeLine = ln.startsWith(">");
              return (
                <div
                  key={ln}
                  className={cn(
                    "whitespace-pre",
                    activeLine ? "text-[var(--soxu-text-primary)] font-medium" : "text-[var(--soxu-text-tertiary)]",
                  )}
                >
                  {ln}
                </div>
              );
            })}
          </div>
        ) : (
          <CodeSnippetView lines={tool.lines} startLine={1} showLineNumbers={showLineNumbers} wrap={wrap} />
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}
