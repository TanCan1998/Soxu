#!/usr/bin/env node
/**
 * Soxu M0.5 设计系统完整性与零 Emoji 结构图标门禁检查
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");

const errors = [];

// 1. 校验 tokens.json 关键扩展块
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

// 2. 校验 MASTER.md 与 6 份分页面规范行数 >= 90
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

// 3. 校验桌面游乐场三文件存在性与 JS 语法
for (const f of ["preview.html", "preview.css", "preview-app.js"]) {
  const p = path.join(repoRoot, "design-system", "soxu", f);
  if (!fs.existsSync(p)) {
    errors.push(`缺失原型文件: design-system/soxu/${f}`);
  }
}

const previewAppPath = path.join(repoRoot, "design-system", "soxu", "preview-app.js");
if (fs.existsSync(previewAppPath)) {
  try {
    execFileSync(process.execPath, ["--check", previewAppPath], { stdio: "pipe" });
  } catch (err) {
    errors.push(`preview-app.js JS 语法校验失败: ${err.message}`);
  }
}

// 4. 校验原型源码禁止使用 Emoji 充当 UI 图标
const emojiRegex = /\p{Extended_Pictographic}/u;
function scanNoEmoji(filePath) {
  const rel = path.relative(repoRoot, filePath);
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
  lines.forEach((line, idx) => {
    if (emojiRegex.test(line)) {
      errors.push(`发现 UI 原型代码中疑似含 Emoji 图标 (${rel}:${idx + 1}): ${line.trim().slice(0, 60)}`);
    }
  });
}

if (fs.existsSync(previewAppPath)) scanNoEmoji(previewAppPath);
const mobileSrcDir = path.join(repoRoot, "apps", "mobile-web", "src");
function walkTs(dir) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) walkTs(full);
    else if (/\.(ts|tsx)$/.test(ent.name)) scanNoEmoji(full);
  }
}
if (fs.existsSync(mobileSrcDir)) walkTs(mobileSrcDir);

if (errors.length > 0) {
  console.error("[design:verify] 校验失败：");
  errors.forEach((e) => console.error(`  - ${e}`));
  process.exit(1);
}

console.log("[design:verify] 令牌、六维规范、原型语法与零 Emoji 门禁全部通过！");
