import { Copy, Quote, Sparkles, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import { isImeComposing } from "@/lib/useAdaptiveViewport";

interface QuoteSelectionPopoverProps {
  containerRef: React.RefObject<HTMLElement | null>;
  isCoarsePointer: boolean;
  onQuote: (snippet: string) => void;
  onCopyText?: (text: string) => void;
}

/**
 * 选区引用回复与悬浮旁路解释卡（吸收 Lynricsy/Oryn 选中即问设计）：
 * - 细指针桌面：浮在选区上方（贴近顶部时自动下翻）；
 * - 触控粗指针：悬在输入舱上方 8px 独立条，避开系统原生选区复制菜单遮挡；
 * - 支持一键「引用回复」注入输入舱，或原地「旁路解释」追问而不打断主回合。
 */
export function QuoteSelectionPopover({
  containerRef,
  isCoarsePointer,
  onQuote,
  onCopyText,
}: QuoteSelectionPopoverProps) {
  const { t } = useI18n();
  const [selectedText, setSelectedText] = useState("");
  const [anchorPos, setAnchorPos] = useState<{ top: number; left: number } | null>(null);
  const [explainOpen, setExplainOpen] = useState(false);
  const [followUp, setFollowUp] = useState("");
  const [extraNote, setExtraNote] = useState<string | null>(null);

  useEffect(() => {
    function readSelection() {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) {
        if (!explainOpen) {
          setSelectedText("");
          setAnchorPos(null);
        }
        return;
      }
      const text = sel.toString().trim();
      if (text.length < 4) {
        if (!explainOpen) {
          setSelectedText("");
          setAnchorPos(null);
        }
        return;
      }
      const root = containerRef.current;
      if (!root || !sel.anchorNode || !root.contains(sel.anchorNode)) return;
      const range = sel.getRangeAt(0);
      const rect = range.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) return;

      const top = rect.top > 96 ? rect.top - 42 : rect.bottom + 8;
      const left = Math.min(window.innerWidth - 220, Math.max(12, rect.left));
      setSelectedText(text);
      setAnchorPos({ top, left });
    }

    const onMouseUp = () => window.setTimeout(readSelection, 16);
    const onSelectionChange = () => {
      if (isCoarsePointer) {
        window.setTimeout(readSelection, 60);
      }
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setExplainOpen(false);
        setSelectedText("");
        setAnchorPos(null);
      }
    };

    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("selectionchange", onSelectionChange);
    window.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("selectionchange", onSelectionChange);
      window.removeEventListener("keydown", onEsc);
    };
  }, [containerRef, explainOpen, isCoarsePointer]);

  function clearAll() {
    setExplainOpen(false);
    setSelectedText("");
    setAnchorPos(null);
    setExtraNote(null);
    setFollowUp("");
    window.getSelection()?.removeAllRanges();
  }

  if (!selectedText) return null;

  const snippet = selectedText.length > 140 ? `${selectedText.slice(0, 140)}...` : selectedText;

  return (
    <>
      {!explainOpen && (
        <div
          style={
            isCoarsePointer || !anchorPos
              ? { bottom: "max(132px, calc(120px + var(--soxu-kb-inset)))", left: "50%", transform: "translateX(-50%)" }
              : { top: `${anchorPos.top}px`, left: `${anchorPos.left}px` }
          }
          className="fixed z-[var(--soxu-z-floating)] rounded-full border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-floating)] px-1.5 py-1 flex items-center gap-1 shadow-[0_8px_24px_rgba(0,0,0,0.6)]"
        >
          <button
            type="button"
            onClick={() => {
              onQuote(selectedText);
              clearAll();
            }}
            className="soxu-hit-44 h-7 px-2.5 rounded-full inline-flex items-center gap-1.5 text-xs font-medium text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-selected)] cursor-pointer"
          >
            <Quote className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
            <span>{t("quote.actionQuote")}</span>
          </button>
          <button
            type="button"
            onClick={() => setExplainOpen(true)}
            className="soxu-hit-44 h-7 px-2.5 rounded-full inline-flex items-center gap-1.5 text-xs font-medium text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-selected)] cursor-pointer"
          >
            <Sparkles className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)]" strokeWidth={1.75} />
            <span>{t("quote.actionExplain")}</span>
          </button>
          <button
            type="button"
            onClick={clearAll}
            aria-label="关闭选区操作"
            className="soxu-hit-44 h-7 w-7 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
          >
            <X className="h-3.5 w-3.5" strokeWidth={1.75} />
          </button>
        </div>
      )}

      {explainOpen && (
        <div
          style={
            isCoarsePointer || !anchorPos
              ? { bottom: "max(132px, calc(120px + var(--soxu-kb-inset)))", left: "50%", transform: "translateX(-50%)" }
              : {
                  top: `${Math.min(window.innerHeight - 280, Math.max(56, anchorPos.top))}px`,
                  left: `${Math.min(window.innerWidth - 360, Math.max(12, anchorPos.left))}px`,
                }
          }
          className="fixed z-[var(--soxu-z-floating)] w-[min(340px,calc(100vw-24px))] rounded-2xl border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-floating)] p-3.5 flex flex-col gap-2.5 shadow-[0_14px_40px_rgba(0,0,0,0.75)]"
        >
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="text-xs font-semibold text-[var(--soxu-text-primary)] flex items-center gap-1.5">
                <Sparkles className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)] shrink-0" strokeWidth={1.75} />
                <span>{t("quote.explainTitle")}</span>
              </div>
              <div className="text-[11px] text-[var(--soxu-text-tertiary)] truncate mt-0.5">{snippet}</div>
            </div>
            <button
              type="button"
              onClick={clearAll}
              aria-label="关闭旁路解释"
              className="soxu-hit-44 h-6 w-6 rounded-full inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer shrink-0"
            >
              <X className="h-3.5 w-3.5" strokeWidth={1.75} />
            </button>
          </div>

          <div className="rounded-xl bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] p-2.5 text-xs leading-relaxed text-[var(--soxu-text-secondary)]">
            {extraNote ??
              `这段内容描述了在受控工作树与安全边界下的执行约束：核心原则是复用主会话前缀缓存做旁路研判，不中断主代理正在执行的工具链。`}
          </div>

          <input
            type="text"
            value={followUp}
            onChange={(e) => setFollowUp(e.target.value)}
            onKeyDown={(e) => {
              if (isImeComposing(e)) return;
              if (e.key === "Enter" && followUp.trim()) {
                e.preventDefault();
                setExtraNote(`针对「${followUp.trim()}」的旁路回答：建议继续保持工作树路径边界并在提交前跑通增量门禁。`);
                setFollowUp("");
              }
            }}
            placeholder={t("quote.explainPlaceholder")}
            className="w-full h-8 rounded-lg border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] px-2.5 text-base sm:text-xs text-[var(--soxu-text-primary)] outline-none placeholder:text-[var(--soxu-text-ghost)]"
          />

          <div className="flex items-center justify-between gap-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                onCopyText?.(extraNote ?? snippet);
                clearAll();
              }}
            >
              <Copy className="h-3 w-3" strokeWidth={1.75} />
              <span>{t("quote.copyAnswer")}</span>
            </Button>
            <Button
              size="sm"
              variant="default"
              onClick={() => {
                onQuote(selectedText);
                clearAll();
              }}
            >
              <Quote className="h-3 w-3" strokeWidth={1.75} />
              <span>{t("quote.convertToQuote")}</span>
            </Button>
          </div>
        </div>
      )}
    </>
  );
}
