#!/usr/bin/env node
/**
 * Soxu 设计令牌同步与防漂移门禁脚本
 * 从唯一真相源 design-system/soxu/tokens.json 编译生成：
 *   1. design-system/soxu/tokens.css
 *   2. apps/mobile-web/src/tokens.generated.css
 *
 * 用法：
 *   node scripts/sync-design-tokens.mjs --write
 *   node scripts/sync-design-tokens.mjs --check
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const tokensJsonPath = path.join(repoRoot, "design-system", "soxu", "tokens.json");
const designCssPath = path.join(repoRoot, "design-system", "soxu", "tokens.css");
const mobileCssPath = path.join(repoRoot, "apps", "mobile-web", "src", "tokens.generated.css");

const mode = process.argv.includes("--check") ? "check" : "write";

const raw = JSON.parse(fs.readFileSync(tokensJsonPath, "utf8"));

function camelToKebab(input) {
  return input
    .replace(/_/g, "-")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase();
}

function emitPaletteVars(palette, indent = "  ") {
  const lines = [];
  for (const [groupName, groupObj] of Object.entries(palette)) {
    if (groupName.startsWith("$")) continue;
    for (const [key, token] of Object.entries(groupObj)) {
      if (key.startsWith("$")) continue;
      lines.push(`${indent}--soxu-${camelToKebab(groupName)}-${camelToKebab(key)}: ${token.$value};`);
    }
  }
  return lines;
}

function buildCssDocument() {
  const lines = [
    "/* ==========================================================================",
    " * GENERATED FROM design-system/soxu/tokens.json — DO NOT EDIT DIRECTLY.",
    " * Run `node scripts/sync-design-tokens.mjs --write` to regenerate.",
    " * ========================================================================== */",
    "",
    ":root,",
    '[data-theme="dark"] {',
    "  color-scheme: dark;",
    ...emitPaletteVars(raw.color.dark),
    "",
    "  /* Typography */",
    `  --soxu-font-sans: ${raw.typography.family.sans.$value};`,
    `  --soxu-font-mono: ${raw.typography.family.mono.$value};`,
    "  --soxu-font-ui: var(--soxu-font-sans);",
    "  --soxu-font-code: var(--soxu-font-mono);",
    "  --soxu-font-code-effective: var(--soxu-font-code);",
    `  --soxu-font-ui-size: ${raw.typography.defaults?.uiFontSizePx ?? 14}px;`,
    `  --soxu-font-code-size: ${raw.typography.defaults?.codeFontSizePx ?? 13}px;`,
    `  --soxu-code-ligatures: ${raw.typography.defaults?.codeLigatures === false ? "0" : '"liga" 1, "calt" 1'};`,
  ];

  for (const [k, v] of Object.entries(raw.typography.scale)) {
    lines.push(`  --soxu-text-${camelToKebab(k)}: ${v.$value};`);
  }

  lines.push("", "  /* Spacing */");
  for (const [k, v] of Object.entries(raw.spacing)) {
    lines.push(`  --soxu-space-${camelToKebab(k)}: ${v.$value};`);
  }

  lines.push("", "  /* Radius */");
  for (const [k, v] of Object.entries(raw.radius)) {
    lines.push(`  --soxu-radius-${camelToKebab(k)}: ${v.$value};`);
  }

  lines.push("", "  /* Layout & Responsive */");
  for (const [k, v] of Object.entries(raw.layout)) {
    lines.push(`  --soxu-layout-${camelToKebab(k)}: ${v.$value};`);
  }

  if (raw.sizing) {
    lines.push("", "  /* Component Sizing */");
    for (const [k, v] of Object.entries(raw.sizing)) {
      lines.push(`  --soxu-size-${camelToKebab(k)}: ${v.$value};`);
    }
  }

  if (raw.zIndex) {
    lines.push("", "  /* Z-Index */");
    for (const [k, v] of Object.entries(raw.zIndex)) {
      lines.push(`  --soxu-z-${camelToKebab(k)}: ${v.$value};`);
    }
  }

  lines.push("", "  /* Motion (1:1 GPUI Portable) */");
  lines.push(`  --soxu-motion-press: ${raw.motion.pressDuration.$value};`);
  lines.push(`  --soxu-motion-hover: ${raw.motion.hoverDuration.$value};`);
  lines.push(`  --soxu-motion-panel: ${raw.motion.panelDuration.$value};`);
  lines.push(`  --soxu-motion-sheet: ${raw.motion.sheetDuration.$value};`);
  if (raw.motion.enterDuration) lines.push(`  --soxu-motion-enter: ${raw.motion.enterDuration.$value};`);
  if (raw.motion.exitDuration) lines.push(`  --soxu-motion-exit: ${raw.motion.exitDuration.$value};`);
  if (raw.motion.hoverExpandDelay) lines.push(`  --soxu-motion-hover-expand-delay: ${raw.motion.hoverExpandDelay.$value};`);
  if (raw.motion.hoverCollapseDelay) lines.push(`  --soxu-motion-hover-collapse-delay: ${raw.motion.hoverCollapseDelay.$value};`);
  if (raw.motion.themeTransitionDuration) lines.push(`  --soxu-motion-theme: ${raw.motion.themeTransitionDuration.$value};`);
  if (raw.motion.themeIconDuration) lines.push(`  --soxu-motion-theme-icon: ${raw.motion.themeIconDuration.$value};`);
  lines.push(`  --soxu-ease-out: cubic-bezier(${raw.motion.easeOut.$value.join(", ")});`);
  lines.push(`  --soxu-ease-in-out: cubic-bezier(${raw.motion.easeInOut.$value.join(", ")});`);
  if (raw.motion.easeInOutCubic) lines.push(`  --soxu-ease-in-out-cubic: cubic-bezier(${raw.motion.easeInOutCubic.$value.join(", ")});`);
  if (raw.motion.easeFast) lines.push(`  --soxu-ease-fast: cubic-bezier(${raw.motion.easeFast.$value.join(", ")});`);

  const emitShadcnMapping = () => [
    "",
    "  /* shadcn/ui Semantic Variable Bridge */",
    "  --background: var(--soxu-surface-canvas);",
    "  --foreground: var(--soxu-text-primary);",
    "  --card: var(--soxu-surface-card);",
    "  --card-foreground: var(--soxu-text-primary);",
    "  --popover: var(--soxu-surface-floating);",
    "  --popover-foreground: var(--soxu-text-primary);",
    "  --primary: var(--soxu-action-inverse);",
    "  --primary-foreground: var(--soxu-action-on-inverse);",
    "  --secondary: var(--soxu-surface-control);",
    "  --secondary-foreground: var(--soxu-text-primary);",
    "  --muted: var(--soxu-surface-card-muted);",
    "  --muted-foreground: var(--soxu-text-secondary);",
    "  --accent: var(--soxu-interaction-selected);",
    "  --accent-foreground: var(--soxu-text-primary);",
    "  --destructive: var(--soxu-semantic-danger);",
    "  --destructive-foreground: #ffffff;",
    "  --border: var(--soxu-border-subtle);",
    "  --input: var(--soxu-border-strong);",
    "  --ring: var(--soxu-border-focus-ring);",
    "  --radius: var(--soxu-radius-md);",
  ];

  lines.push(...emitShadcnMapping(), "}", "");

  lines.push('[data-theme="light"] {', "  color-scheme: light;", ...emitPaletteVars(raw.color.light), ...emitShadcnMapping(), "}", "");

  lines.push(
    "@media (prefers-reduced-motion: reduce) {",
    "  :root {",
    "    --soxu-motion-press: 0ms;",
    "    --soxu-motion-hover: 0ms;",
    "    --soxu-motion-panel: 0ms;",
    "    --soxu-motion-sheet: 0ms;",
    "    --soxu-motion-enter: 0ms;",
    "    --soxu-motion-exit: 0ms;",
    "    --soxu-motion-theme: 0ms;",
    "    --soxu-motion-theme-icon: 0ms;",
    "  }",
    "}",
    ""
  );

  return lines.join("\n");
}

const expectedCss = buildCssDocument();
const targets = [designCssPath, mobileCssPath];

if (mode === "check") {
  let failed = false;
  for (const target of targets) {
    if (!fs.existsSync(target)) {
      console.error(`[tokens:check] 缺失生成文件: ${path.relative(repoRoot, target)}`);
      failed = true;
      continue;
    }
    const actual = fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n");
    if (actual !== expectedCss) {
      console.error(
        `[tokens:check] 检测到令牌漂移: ${path.relative(repoRoot, target)}，请运行 node scripts/sync-design-tokens.mjs --write`
      );
      failed = true;
    }
  }
  if (failed) {
    process.exit(1);
  }
  console.log("[tokens:check] 双端设计令牌完全同步，无漂移。");
} else {
  for (const target of targets) {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, expectedCss, "utf8");
    console.log(`[tokens:write] 已生成 ${path.relative(repoRoot, target)}`);
  }
}
