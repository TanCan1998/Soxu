#!/usr/bin/env node
/**
 * Soxu M0.5 设计系统完整性、shadcn/ui 组件门禁与零 Emoji UI 纪律校验
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

const errors = [];

// 1. 校验 tokens.json 关键扩展块（纯黑色彩、字体预设、GPUI 动效与尺寸）
const tokensPath = path.join(repoRoot, "design-system", "soxu", "tokens.json");
const tokens = JSON.parse(fs.readFileSync(tokensPath, "utf8"));
for (const mode of ["dark", "light"]) {
  if (!tokens.color?.[mode]?.syntax || Object.keys(tokens.color[mode].syntax).length < 14) {
    errors.push(`tokens.json 缺少完整的 color.${mode}.syntax 令牌组`);
  }
  if (!tokens.color?.[mode]?.ansi || Object.keys(tokens.color[mode].ansi).length < 16) {
    errors.push(`tokens.json 缺少完整的 color.${mode}.ansi 16 色组`);
  }
}
if (!tokens.sizing || Object.keys(tokens.sizing).length < 12) {
  errors.push("tokens.json 缺少 sizing 组件几何尺寸令牌");
}
if (!tokens.zIndex || Object.keys(tokens.zIndex).length < 8) {
  errors.push("tokens.json 缺少 zIndex 层叠顺序令牌");
}
if (!tokens.typography?.uiPresets || Object.keys(tokens.typography.uiPresets).length < 5) {
  errors.push("tokens.json 缺少不少于 5 项的 typography.uiPresets 开源界面字体预设");
}
if (!tokens.typography?.codePresets || Object.keys(tokens.typography.codePresets).length < 5) {
  errors.push("tokens.json 缺少不少于 5 项的 typography.codePresets 开源代码字体预设");
}
if (!tokens.color?.dark?.interaction?.scrollbarThumb || !tokens.color?.light?.interaction?.scrollbarThumb) {
  errors.push("tokens.json 缺少深浅双模式滚动条滑块颜色令牌 scrollbarThumb");
}
if (!tokens.accentPresets || Object.keys(tokens.accentPresets).length < 6) {
  errors.push("tokens.json 缺少不少于 6 项的 accentPresets 主题强调色预设");
}
if (!tokens.i18n?.defaultLocale || !Array.isArray(tokens.i18n?.supportedLocales) || tokens.i18n.supportedLocales.length < 3) {
  errors.push("tokens.json 缺少 i18n 默认语言与支持语言清单配置");
}
for (const motionKey of [
  "enterDuration",
  "exitDuration",
  "hoverExpandDelay",
  "hoverCollapseDelay",
  "themeTransitionDuration",
  "themeIconDuration",
  "easeOutExpo",
  "easeInOutCubic",
]) {
  if (!tokens.motion?.[motionKey]) {
    errors.push(`tokens.json 缺少动效令牌 motion.${motionKey}`);
  }
}

// 2. 校验 MASTER.md 与 7 份分页面规范行数 >= 90
const masterPath = path.join(repoRoot, "design-system", "soxu", "MASTER.md");
if (!fs.existsSync(masterPath) || fs.readFileSync(masterPath, "utf8").split(/\r?\n/).length < 120) {
  errors.push("design-system/soxu/MASTER.md 缺失或实质内容不足 120 行");
}

const requiredPages = [
  "01-desktop-workbench.md",
  "02-composer-and-handoff.md",
  "03-timeline-tools-and-checkpoints.md",
  "04-magpie-and-cli-inspector.md",
  "05-mobile-remote-pwa.md",
  "06-modals-onboarding-and-automation.md",
  "07-settings-fonts-and-motion.md",
];
for (const page of requiredPages) {
  const p = path.join(repoRoot, "design-system", "soxu", "pages", page);
  if (!fs.existsSync(p)) {
    errors.push(`缺失分页面规范: design-system/soxu/pages/${page}`);
    continue;
  }
  const lineCount = fs.readFileSync(p, "utf8").split(/\r?\n/).length;
  if (lineCount < 90) {
    errors.push(`规范 ${page} 当前仅 ${lineCount} 行，低于最低 90 行精细标准`);
  }
}

// 3. 校验 shadcn/ui + Tailwind v4 原型工程完整性与旧文件清理
const webRoot = path.join(repoRoot, "apps", "mobile-web");
const requiredFiles = [
  "components.json",
  "desktop.html",
  "vite.preview.config.ts",
  "src/main.tsx",
  "src/desktop-main.tsx",
  "src/desktop/DesktopWorkbench.tsx",
  "src/lib/utils.ts",
  "src/lib/i18n.ts",
  "src/lib/preferences.ts",
  "src/lib/agent-capabilities.ts",
  "src/components/ThemeToggle.tsx",
  "src/components/AgentIcon.tsx",
  "src/components/ComposerActionMenus.tsx",
  "src/components/HoverExpandDock.tsx",
  "src/components/ContextUsageRing.tsx",
  "src/components/SessionStatsPopover.tsx",
  "src/components/CodeBlockView.tsx",
  "src/components/GitCommitDialog.tsx",
  "src/components/SettingsDialog.tsx",
  "src/components/UtilityModals.tsx",
  ...[
    "badge.tsx",
    "button.tsx",
    "collapsible.tsx",
    "dialog.tsx",
    "dropdown-menu.tsx",
    "select.tsx",
    "separator.tsx",
    "sheet.tsx",
    "switch.tsx",
    "tabs.tsx",
    "tooltip.tsx",
  ].map((u) => `src/components/ui/${u}`),
];
for (const rel of requiredFiles) {
  if (!fs.existsSync(path.join(webRoot, rel))) {
    errors.push(`缺失 shadcn/ui 前端原型文件: apps/mobile-web/${rel}`);
  }
}

const forbiddenLegacyFiles = [
  "design-system/soxu/preview.css",
  "design-system/soxu/preview-app.js",
  "apps/mobile-web/src/icons.tsx",
  "apps/mobile-web/src/components/BottomSheet.tsx",
];
for (const rel of forbiddenLegacyFiles) {
  if (fs.existsSync(path.join(repoRoot, rel))) {
    errors.push(`检测到已废弃的旧版原型残留文件: ${rel}`);
  }
}

// 4. 校验离线单文件桌面工作台预览 design-system/soxu/preview.html
const previewHtmlPath = path.join(repoRoot, "design-system", "soxu", "preview.html");
if (!fs.existsSync(previewHtmlPath)) {
  errors.push("缺失单文件离线桌面预览产物: design-system/soxu/preview.html（请执行 pnpm run build:preview）");
} else {
  const stat = fs.statSync(previewHtmlPath);
  if (stat.size < 50 * 1024) {
    errors.push(`design-system/soxu/preview.html 体积过小 (${stat.size} B)，疑似非完整单文件打包产物`);
  }
  if (stat.size > 5 * 1024 * 1024) {
    errors.push(`design-system/soxu/preview.html 体积超出 5MB 仓库上限 (${stat.size} B)`);
  }
}

// 5. 校验前端 TypeScript / TSX 源码禁止使用 Emoji 充当 UI 图标，且严禁将品牌名 Soxu 翻译为“索序”
const emojiRegex = /\p{Extended_Pictographic}/u;
function scanUiSource(filePath) {
  const rel = path.relative(repoRoot, filePath);
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
  lines.forEach((line, idx) => {
    if (emojiRegex.test(line)) {
      errors.push(`发现 UI 源码中疑似含 Emoji 图标 (${rel}:${idx + 1}): ${line.trim().slice(0, 60)}`);
    }
    if (line.includes("索序")) {
      errors.push(`品牌名 Soxu 严禁中文翻译 (${rel}:${idx + 1}): ${line.trim().slice(0, 60)}`);
    }
  });
}

const mobileSrcDir = path.join(webRoot, "src");
function walkTs(dir) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) walkTs(full);
    else if (/\.(ts|tsx)$/.test(ent.name)) scanUiSource(full);
  }
}
if (fs.existsSync(mobileSrcDir)) walkTs(mobileSrcDir);

for (const htmlRel of ["desktop.html", "index.html", "public/manifest.webmanifest"]) {
  const htmlPath = path.join(webRoot, htmlRel);
  if (fs.existsSync(htmlPath) && fs.readFileSync(htmlPath, "utf8").includes("索序")) {
    errors.push(`HTML/PWA 清单中严禁将品牌名 Soxu 翻译为“索序”: apps/mobile-web/${htmlRel}`);
  }
}

console.log(
  errors.length === 0
    ? "[design:verify] 令牌、七维规范、shadcn/ui 组件、品牌词纪律、单文件预览与零 Emoji 门禁全部通过！"
    : "[design:verify] 检测结束（即将汇总错误）",
);

if (errors.length > 0) {
  console.error("[design:verify] 校验失败：");
  errors.forEach((e) => console.error(`  - ${e}`));
  process.exit(1);
}
