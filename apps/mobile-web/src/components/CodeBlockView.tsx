import { cn } from "@/lib/utils";

export interface DiffRowItem {
  lineNum?: number;
  kind: "ctx" | "add" | "del";
  code: string;
}

interface CodeSnippetViewProps {
  lines: string[];
  startLine?: number;
  showLineNumbers?: boolean;
  wrap?: boolean;
  className?: string;
}

/**
 * 普通代码片段渲染器：
 * - 行号列严格右对齐，连续唯一递增，绝不重复；
 * - 未开启自动折行时，仅在整个代码块最底部提供唯一一条整体横向滚动条，杜绝逐行出现滚动条；
 * - 横向滚动时行号列采用粘性定位配合不透明底色防止代码透叠。
 */
export function CodeSnippetView({
  lines,
  startLine = 1,
  showLineNumbers = true,
  wrap = false,
  className,
}: CodeSnippetViewProps) {
  return (
    <div
      className={cn(
        "rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] overflow-hidden soxu-code-block leading-relaxed",
        className,
      )}
    >
      <div className={cn("w-full", !wrap && "overflow-x-auto")}>
        <div className={cn("flex flex-col", !wrap && "min-w-full w-max")}>
          {lines.map((line, idx) => {
            const lineNo = startLine + idx;
            return (
              <div key={lineNo} className="flex items-stretch">
                {showLineNumbers && (
                  <span
                    className={cn(
                      "w-[42px] shrink-0 px-2 py-0.5 text-right text-[var(--soxu-text-tertiary)] border-r border-[var(--soxu-border-hairline)] select-none soxu-tabular text-[11px] bg-[var(--soxu-surface-terminal)]",
                      !wrap && "sticky left-0 z-10",
                    )}
                  >
                    {lineNo}
                  </span>
                )}
                <span
                  className={cn(
                    "px-2.5 py-0.5 flex-1 text-[var(--soxu-text-code)]",
                    wrap ? "min-w-0 whitespace-pre-wrap break-all" : "whitespace-pre",
                  )}
                >
                  {line}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

interface CodeDiffViewProps {
  rows: DiffRowItem[];
  layout?: "unified" | "split";
  showLineNumbers?: boolean;
  wrap?: boolean;
  className?: string;
}

/**
 * 代码差异渲染器：
 * - 整个 Diff 块仅在最底部提供唯一一条整体横向滚动条（单栏/双栏均不按单行渲染滚动条）；
 * - 单栏模式左侧固定单列右对齐行号槽（删除行显示减号，避免重复行号），粘性防透底；
 * - 双栏模式左右两列各自保有独立的右对齐唯一递增行号槽。
 */
export function CodeDiffView({
  rows,
  layout = "unified",
  showLineNumbers = true,
  wrap = false,
  className,
}: CodeDiffViewProps) {
  if (layout === "split") {
    let oldLine = 22;
    let newLine = 22;
    const paired = rows.map((r, idx) => {
      if (r.kind === "ctx") {
        const o = oldLine++;
        const n = newLine++;
        return { key: `s-${idx}`, leftNum: o, rightNum: n, left: r, right: r };
      }
      if (r.kind === "del") {
        const o = oldLine++;
        return { key: `s-${idx}`, leftNum: o, rightNum: null, left: r, right: null };
      }
      const n = newLine++;
      return { key: `s-${idx}`, leftNum: null, rightNum: n, left: null, right: r };
    });

    return (
      <div
        className={cn(
          "rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-inset)] overflow-hidden soxu-code-block text-[11.5px] leading-relaxed",
          className,
        )}
      >
        <div className={cn("w-full", !wrap && "overflow-x-auto")}>
          <div className={cn("flex flex-col", !wrap && "min-w-full w-max")}>
            {paired.map((pair) => (
              <div
                key={pair.key}
                className={cn(
                  "grid border-b last:border-b-0 border-[var(--soxu-border-hairline)]",
                  wrap ? "grid-cols-2" : "grid-cols-[minmax(220px,1fr)_minmax(220px,1fr)]",
                )}
              >
                <div
                  className="flex min-w-0 border-r border-[var(--soxu-border-hairline)]"
                  style={{
                    backgroundColor: pair.left?.kind === "del" ? "var(--soxu-syntax-diff-del-bg)" : undefined,
                  }}
                >
                  {showLineNumbers && (
                    <span className="w-[38px] shrink-0 px-1.5 py-0.5 text-right text-[var(--soxu-text-tertiary)] border-r border-[var(--soxu-border-hairline)] select-none soxu-tabular">
                      {pair.leftNum ?? ""}
                    </span>
                  )}
                  <span
                    className={cn(
                      "px-2 py-0.5 flex-1",
                      wrap ? "min-w-0 whitespace-pre-wrap break-all" : "whitespace-pre",
                      pair.left?.kind === "del" ? "text-[var(--soxu-semantic-danger)]" : "text-[var(--soxu-text-secondary)]",
                    )}
                  >
                    {pair.left ? pair.left.code : ""}
                  </span>
                </div>

                <div
                  className="flex min-w-0"
                  style={{
                    backgroundColor: pair.right?.kind === "add" ? "var(--soxu-syntax-diff-add-bg)" : undefined,
                  }}
                >
                  {showLineNumbers && (
                    <span className="w-[38px] shrink-0 px-1.5 py-0.5 text-right text-[var(--soxu-text-tertiary)] border-r border-[var(--soxu-border-hairline)] select-none soxu-tabular">
                      {pair.rightNum ?? ""}
                    </span>
                  )}
                  <span
                    className={cn(
                      "px-2 py-0.5 flex-1",
                      wrap ? "min-w-0 whitespace-pre-wrap break-all" : "whitespace-pre",
                      pair.right?.kind === "add" ? "text-[var(--soxu-semantic-success)]" : "text-[var(--soxu-text-secondary)]",
                    )}
                  >
                    {pair.right ? pair.right.code : ""}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  let runningNew = 22;
  return (
    <div
      className={cn(
        "rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-inset)] overflow-hidden soxu-code-block leading-relaxed",
        className,
      )}
    >
      <div className={cn("w-full", !wrap && "overflow-x-auto")}>
        <div className={cn("flex flex-col", !wrap && "min-w-full w-max")}>
          {rows.map((r, idx) => {
            const displayNum = r.kind === "del" ? "−" : String(r.lineNum ?? runningNew++);
            if (r.kind === "ctx" && r.lineNum !== undefined) {
              runningNew = r.lineNum + 1;
            }
            const gutterTint =
              r.kind === "add"
                ? "linear-gradient(var(--soxu-syntax-diff-add-gutter), var(--soxu-syntax-diff-add-gutter))"
                : r.kind === "del"
                  ? "linear-gradient(var(--soxu-syntax-diff-del-gutter), var(--soxu-syntax-diff-del-gutter))"
                  : undefined;

            return (
              <div
                key={idx}
                className="flex items-stretch"
                style={{
                  backgroundColor:
                    r.kind === "add"
                      ? "var(--soxu-syntax-diff-add-bg)"
                      : r.kind === "del"
                        ? "var(--soxu-syntax-diff-del-bg)"
                        : undefined,
                }}
              >
                {showLineNumbers && (
                  <span
                    className={cn(
                      "w-[42px] shrink-0 px-2 py-0.5 text-right text-[var(--soxu-text-tertiary)] border-r border-[var(--soxu-border-hairline)] select-none soxu-tabular text-[11px] bg-[var(--soxu-surface-inset)]",
                      !wrap && "sticky left-0 z-10",
                    )}
                    style={gutterTint ? { backgroundImage: gutterTint } : undefined}
                  >
                    {displayNum}
                  </span>
                )}
                <span
                  className={cn(
                    "w-5 shrink-0 py-0.5 text-center select-none",
                    r.kind === "add"
                      ? "text-[var(--soxu-semantic-success)]"
                      : r.kind === "del"
                        ? "text-[var(--soxu-semantic-danger)]"
                        : "text-[var(--soxu-text-ghost)]",
                  )}
                >
                  {r.kind === "add" ? "+" : r.kind === "del" ? "-" : " "}
                </span>
                <span
                  className={cn(
                    "pr-3 py-0.5 flex-1",
                    wrap ? "min-w-0 whitespace-pre-wrap break-all" : "whitespace-pre",
                    r.kind === "add"
                      ? "text-[var(--soxu-semantic-success)]"
                      : r.kind === "del"
                        ? "text-[var(--soxu-semantic-danger)]"
                        : "text-[var(--soxu-syntax-plain)]",
                  )}
                >
                  {r.code.replace(/^[+-]\s?/, "")}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
