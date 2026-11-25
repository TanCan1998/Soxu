import { Check, ChevronDown, GitCommit, Sparkles } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

interface CommitTypeOption {
  emoji: string;
  type: string;
  label: string;
}

const COMMIT_TYPES: CommitTypeOption[] = [
  { emoji: "\u2728", type: "feat", label: "新功能" },
  { emoji: "\ud83d\udc1b", type: "fix", label: "修缺陷" },
  { emoji: "\ud83d\udc84", type: "ui", label: "样式优化" },
  { emoji: "\u267b\ufe0f", type: "refactor", label: "重构" },
  { emoji: "\ud83d\udcdd", type: "docs", label: "文档" },
  { emoji: "\ud83d\udd27", type: "chore", label: "工程配置" },
  { emoji: "\u26a1", type: "perf", label: "性能优化" },
  { emoji: "\u2705", type: "test", label: "测试" },
];

const SCOPE_PRESETS = [
  { value: "", label: "无范围" },
  { value: "drivers", label: "drivers (驱动层)" },
  { value: "core", label: "core (核心状态)" },
  { value: "desktop", label: "desktop (桌面端)" },
  { value: "mobile", label: "mobile (移动端)" },
  { value: "protocol", label: "protocol (协议层)" },
  { value: "git", label: "git (版本与工作树)" },
  { value: "design", label: "design (设计系统)" },
  { value: "ci", label: "ci (流水线)" },
];

interface GitCommitDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  branch?: string;
  onFlashToast: (msg: string) => void;
}

/**
 * 结构化 Git 提交编辑器对话框：
 * 将规范表情与类型、括号提交范围（预设下拉 + 自由输入）、简述与正文拆分为高效可控区域，并保留充裕边距。
 */
export function GitCommitDialog({ open, onOpenChange, branch = "feat/12-native-drivers", onFlashToast }: GitCommitDialogProps) {
  const [selectedType, setSelectedType] = useState<CommitTypeOption>(COMMIT_TYPES[0]);
  const [scope, setScope] = useState("drivers");
  const [summary, setSummary] = useState("补齐原生驱动热纠偏能力探测与临时目录单元测试");
  const [body, setBody] = useState(
    "关联议题 #12\n\n- 新增 supports_live_steer 接口探测各驱动热注入能力\n- 所有测试夹具隔离于临时目录，禁止接触全局目录",
  );
  const [files, setFiles] = useState([
    { path: "crates/soxu-drivers/src/lib.rs", stat: "+28 -3", checked: true },
    { path: "crates/soxu-drivers/tests/steer_matrix.rs", stat: "+42 -0", checked: true },
  ]);

  const cleanScope = scope.trim().replace(/[()]/g, "");
  const headerPreview = `${selectedType.emoji} ${selectedType.type}${cleanScope ? `(${cleanScope})` : ""}: ${summary.trim() || "请填写提交简述"}`;
  const checkedCount = files.filter((f) => f.checked).length;

  function handleAutoDraft() {
    setSelectedType(COMMIT_TYPES[0]);
    setScope("drivers");
    setSummary("补齐热纠偏支持检测与隔离目录单测用例");
    onFlashToast("已由轻量模型生成规范提交标题");
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[min(640px,95vw)] p-6 flex flex-col gap-5 max-h-[88dvh] overflow-y-auto soxu-scroll">
        <DialogHeader className="gap-1.5">
          <DialogTitle className="flex items-center gap-2 text-base">
            <GitCommit className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
            <span>提交工作树变更</span>
            <Badge variant="neutral" className="soxu-mono ml-1">
              {branch}
            </Badge>
          </DialogTitle>
          <DialogDescription>
            选择规范类型图标与括号范围（可自定义），并在下方编辑器完善提交说明。
          </DialogDescription>
        </DialogHeader>

        {/* 暂存文件确认列表 */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between text-xs text-[var(--soxu-text-secondary)]">
            <span className="font-medium">纳入本次提交的文件 ({checkedCount}/{files.length})</span>
            <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={handleAutoDraft}>
              <Sparkles className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
              <span>智能生成草稿</span>
            </Button>
          </div>
          <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] divide-y divide-[var(--soxu-border-hairline)]">
            {files.map((f) => (
              <label
                key={f.path}
                className="px-3.5 py-2.5 flex items-center justify-between gap-3 text-xs cursor-pointer hover:bg-[var(--soxu-interaction-hover)] transition-colors"
              >
                <span className="flex items-center gap-2.5 min-w-0">
                  <input
                    type="checkbox"
                    checked={f.checked}
                    onChange={() =>
                      setFiles((prev) => prev.map((x) => (x.path === f.path ? { ...x, checked: !x.checked } : x)))
                    }
                    className="h-3.5 w-3.5 accent-[var(--soxu-semantic-accent)] cursor-pointer"
                  />
                  <span className="soxu-mono truncate text-[var(--soxu-text-primary)]">{f.path}</span>
                </span>
                <span className="soxu-mono soxu-tabular text-[var(--soxu-semantic-success)] shrink-0">{f.stat}</span>
              </label>
            ))}
          </div>
        </div>

        {/* 快速选择提交表情/类型 */}
        <div className="flex flex-col gap-2">
          <div className="text-xs font-medium text-[var(--soxu-text-secondary)]">表情与提交类型</div>
          <div className="grid grid-cols-4 sm:grid-cols-8 gap-1.5">
            {COMMIT_TYPES.map((ct) => {
              const active = selectedType.type === ct.type;
              return (
                <button
                  key={ct.type}
                  type="button"
                  onClick={() => setSelectedType(ct)}
                  className={cn(
                    "rounded-lg border px-2 py-2 flex flex-col items-center gap-1 text-[11px] cursor-pointer transition-colors",
                    active
                      ? "border-[var(--soxu-border-focus-ring)] bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)] font-semibold"
                      : "border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-strong)]",
                  )}
                >
                  <span className="text-sm leading-none">{ct.emoji}</span>
                  <span>{ct.label}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* 提交范围（可自定义 + 快速下拉）与简述 */}
        <div className="grid grid-cols-1 sm:grid-cols-[200px_1fr] gap-3.5">
          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-[var(--soxu-text-secondary)]" htmlFor="commit-scope-input">
              提交范围（括号内）
            </label>
            <div className="flex items-center rounded-lg border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] focus-within:border-[var(--soxu-border-focus-ring)] overflow-hidden h-9">
              <input
                id="commit-scope-input"
                value={scope}
                onChange={(e) => setScope(e.target.value)}
                placeholder="自定义或留空"
                className="flex-1 min-w-0 bg-transparent border-0 px-3 text-xs soxu-mono text-[var(--soxu-text-primary)] outline-none placeholder:text-[var(--soxu-text-ghost)]"
              />
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    aria-label="选择常用范围"
                    className="h-full px-2 border-l border-[var(--soxu-border-subtle)] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-interaction-hover)] cursor-pointer transition-colors"
                  >
                    <ChevronDown className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-52 max-h-64 overflow-y-auto soxu-scroll">
                  <DropdownMenuLabel>快速选择模块范围</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  {SCOPE_PRESETS.map((sp) => (
                    <DropdownMenuItem key={sp.label} onClick={() => setScope(sp.value)}>
                      <span className="text-xs">{sp.label}</span>
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>

          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-[var(--soxu-text-secondary)]" htmlFor="commit-summary-input">
              提交简述
            </label>
            <input
              id="commit-summary-input"
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              placeholder="简明扼要描述本次变更..."
              className="h-9 w-full rounded-lg border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] px-3 text-xs text-[var(--soxu-text-primary)] outline-none focus:border-[var(--soxu-border-focus-ring)]"
            />
          </div>
        </div>

        {/* 合成标题预览与详细正文编辑框 */}
        <div className="flex flex-col gap-2">
          <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] px-3.5 py-2.5 text-xs soxu-code-block text-[var(--soxu-text-primary)]">
            {headerPreview}
          </div>
          <label className="text-xs font-medium text-[var(--soxu-text-secondary)] mt-1" htmlFor="commit-body-textarea">
            详细说明（可选）
          </label>
          <textarea
            id="commit-body-textarea"
            rows={4}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="补充改动动机、测试结论或关联议题编号..."
            className="w-full rounded-xl border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] p-3 text-xs leading-relaxed text-[var(--soxu-text-primary)] outline-none focus:border-[var(--soxu-border-focus-ring)] resize-y soxu-scroll"
          />
        </div>

        {/* 门禁校验状态与底栏按钮 */}
        <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
          <div className="flex flex-wrap gap-1.5">
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> 表情格式规范
            </Badge>
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> 无个人路径泄露
            </Badge>
            <Badge variant="accent">
              <Check className="h-3 w-3" strokeWidth={2} /> 密钥扫描通过
            </Badge>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              取消
            </Button>
            <Button
              variant="default"
              disabled={checkedCount === 0 || !summary.trim()}
              onClick={() => {
                onOpenChange(false);
                onFlashToast(`已提交 ${checkedCount} 个文件：${headerPreview}`);
              }}
            >
              确认提交 ({checkedCount})
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
