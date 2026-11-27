#!/usr/bin/env node
/**
 * Soxu 设计令牌同步与防漂移门禁脚本
 * 从唯一真相源 design-system/soxu/tokens.json 编译生成三份同源产物：
 *   1. design-system/soxu/tokens.css
 *   2. apps/mobile-web/src/tokens.generated.css
 *   3. crates/soxu-desktop/src/theme/tokens.rs
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
const rustTokensPath = path.join(repoRoot, "crates", "soxu-desktop", "src", "theme", "tokens.rs");

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

/* -------------------------------------------------------------------------- */
/* Rust 主题常量编译（crates/soxu-desktop/src/theme/tokens.rs）                 */
/* -------------------------------------------------------------------------- */

/** tokens.json 的键 → Rust SCREAMING_SNAKE_CASE 常量名主体。 */
function screamingSnake(input) {
  return String(input)
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[^0-9A-Za-z]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .toUpperCase();
}

/** 解析 #RRGGBB / rgb() / rgba() 令牌，返回紧凑十六进制字面量与 alpha 通道。 */
function parseColorToken(value, jsonPath) {
  const compact = String(value).replace(/\s+/g, "");
  if (/^#[0-9a-fA-F]{6}$/.test(compact)) {
    return { literal: `0x${compact.slice(1).toLowerCase()}`, alpha: 0xff };
  }
  const call = /^rgba?\(([^)]*)\)$/.exec(compact);
  if (!call) {
    throw new Error(`[tokens] 无法解析颜色令牌 ${jsonPath}: ${value}`);
  }
  const parts = call[1].split(",");
  if (parts.length < 3 || parts.length > 4) {
    throw new Error(`[tokens] 颜色通道数量异常 ${jsonPath}: ${value}`);
  }
  const channel = (rawPart) => {
    const n = Number(rawPart);
    if (!Number.isFinite(n) || n < 0 || n > 255) {
      throw new Error(`[tokens] 颜色通道越界 ${jsonPath}: ${value}`);
    }
    return Math.round(n);
  };
  const r = channel(parts[0]);
  const g = channel(parts[1]);
  const b = channel(parts[2]);
  let alpha = 0xff;
  if (parts.length === 4) {
    const a = Number(parts[3]);
    if (!Number.isFinite(a) || a < 0 || a > 1) {
      throw new Error(`[tokens] 颜色 alpha 越界 ${jsonPath}: ${value}`);
    }
    alpha = Math.round(a * 255);
  }
  const rgb = ((r << 16) | (g << 8) | b).toString(16).padStart(6, "0");
  return {
    literal: alpha === 0xff ? `0x${rgb}` : `0x${rgb}${alpha.toString(16).padStart(2, "0")}`,
    alpha,
  };
}

/** 解析 `Npx` 尺寸令牌。 */
function parsePxToken(value, jsonPath) {
  const match = /^(-?\d+(?:\.\d+)?)px$/.exec(String(value).trim());
  if (!match) {
    throw new Error(`[tokens] 无法解析尺寸令牌 ${jsonPath}: ${value}`);
  }
  return Number(match[1]);
}

/** 解析 `Nms` 动效时长令牌，返回整毫秒。 */
function parseMsToken(value, jsonPath) {
  const match = /^(\d+(?:\.\d+)?)ms$/.exec(String(value).trim());
  if (!match) {
    throw new Error(`[tokens] 无法解析动效时长令牌 ${jsonPath}: ${value}`);
  }
  const n = Number(match[1]);
  if (!Number.isInteger(n)) {
    throw new Error(`[tokens] 动效时长不是整毫秒 ${jsonPath}: ${value}`);
  }
  return n;
}

/** 去掉常量名尾部已存在的 `_PX`，避免 `desktopBasePx` 生成 `_PX_PX`。 */
function withPxSuffix(constName) {
  return constName.endsWith("_PX") ? constName : `${constName}_PX`;
}

/** Rust f32 字面量：整数补齐 `.0`，小数保留原始精度。 */
function formatF32(n) {
  const text = String(n);
  return text.includes(".") ? text : `${text}.0`;
}

/** Rust 字符串字面量转义。 */
function rustStringLiteral(value) {
  const escaped = String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}"`;
}

/** 仓库无 rustfmt.toml 时 rustfmt 默认行宽。 */
const RUSTFMT_MAX_WIDTH = 100;

/**
 * 生成 `pub const X: &str = "...";`，并按 rustfmt 的换行规则决定是否在 `=` 后折行：
 * 单行超宽且折行后的字面量能装进行宽时才折行，否则 rustfmt 会保留单行长字符串。
 */
function emitStrConst(lines, indent, name, value) {
  const literal = rustStringLiteral(value);
  const singleLine = `${indent}pub const ${name}: &str = ${literal};`;
  if (singleLine.length <= RUSTFMT_MAX_WIDTH) {
    lines.push(singleLine);
    return;
  }
  const wrappedLine = `${indent}    ${literal};`;
  if (wrappedLine.length <= RUSTFMT_MAX_WIDTH) {
    lines.push(`${indent}pub const ${name}: &str =`);
    lines.push(wrappedLine);
    return;
  }
  lines.push(singleLine);
}

function rustDocLine(lines, indent, jsonPath, note) {
  lines.push(`${indent}/// 源令牌 \`${jsonPath}\`${note ?? ""}。`);
}

function emitColorConst(lines, indent, name, jsonPath, token) {
  const { literal, alpha } = parseColorToken(token.$value, jsonPath);
  rustDocLine(lines, indent, jsonPath, alpha === 0xff ? "" : "（含 alpha，格式 `0xRRGGBBAA`）");
  lines.push(`${indent}pub const ${name}: u32 = ${literal};`);
}

function emitPxConst(lines, indent, name, jsonPath, token) {
  const px = parsePxToken(token.$value, jsonPath);
  rustDocLine(lines, indent, jsonPath);
  lines.push(`${indent}pub const ${name}: f32 = ${formatF32(px)};`);
}

function emitMsConst(lines, indent, name, jsonPath, token) {
  const ms = parseMsToken(token.$value, jsonPath);
  rustDocLine(lines, indent, jsonPath);
  lines.push(`${indent}pub const ${name}: u64 = ${ms};`);
}

function emitNumberConst(lines, indent, name, jsonPath, token, typeName = "u32") {
  const n = Number(token.$value);
  if (!Number.isFinite(n)) {
    throw new Error(`[tokens] 数值令牌无效 ${jsonPath}: ${token.$value}`);
  }
  if (typeName === "u32" && !Number.isInteger(n)) {
    throw new Error(`[tokens] 期望整数的令牌出现小数 ${jsonPath}: ${token.$value}`);
  }
  rustDocLine(lines, indent, jsonPath);
  const literal = typeName === "f32" ? formatF32(n) : String(n);
  lines.push(`${indent}pub const ${name}: ${typeName} = ${literal};`);
}

function emitCubicBezierConst(lines, indent, name, jsonPath, token) {
  const values = token.$value;
  if (!Array.isArray(values) || values.length !== 4 || values.some((v) => !Number.isFinite(Number(v)))) {
    throw new Error(`[tokens] 贝塞尔曲线令牌无效 ${jsonPath}: ${JSON.stringify(values)}`);
  }
  rustDocLine(lines, indent, jsonPath);
  lines.push(`${indent}pub const ${name}: [f32; 4] = [${values.map((v) => formatF32(Number(v))).join(", ")}];`);
}

/** 按令牌组展开为扁平常量块；emitToken 决定具体类型与后缀。 */
function emitFlatGroup(lines, options) {
  const { title, moduleName, jsonPath, group, prefix = "", emitToken, indent = "    " } = options;
  lines.push(`/// ${title}`, `pub mod ${moduleName} {`);
  for (const [key, token] of Object.entries(group)) {
    if (key.startsWith("$") || !token || typeof token !== "object" || !("$value" in token)) {
      continue;
    }
    emitToken(lines, indent, `${prefix}${screamingSnake(key)}`, `${jsonPath}.${key}`, token);
  }
  lines.push("}", "");
}

function emitTypographySection(lines) {
  const jsonPath = "typography";
  lines.push(
    "/// 排版令牌：字体栈、基准字号、字重开关与字号阶梯。",
    "///",
    "/// 字号一律以 CSS px 表达（`_PX`），GPUI 侧按需换算为 rems。",
    "pub mod typography {"
  );

  const { family, defaults, scale } = raw.typography;
  for (const [key, token] of Object.entries(family)) {
    if (key.startsWith("$")) continue;
    rustDocLine(lines, "    ", `${jsonPath}.family.${key}`, "（字体栈）");
    emitStrConst(lines, "    ", `FONT_${screamingSnake(key)}`, token.$value);
  }
  if (defaults) {
    if (defaults.uiFontSizePx !== undefined) {
      rustDocLine(lines, "    ", `${jsonPath}.defaults.uiFontSizePx`);
      lines.push(`    pub const FONT_UI_SIZE_PX: f32 = ${formatF32(Number(defaults.uiFontSizePx))};`);
    }
    if (defaults.codeFontSizePx !== undefined) {
      rustDocLine(lines, "    ", `${jsonPath}.defaults.codeFontSizePx`);
      lines.push(`    pub const FONT_CODE_SIZE_PX: f32 = ${formatF32(Number(defaults.codeFontSizePx))};`);
    }
    if (defaults.codeLigatures !== undefined) {
      rustDocLine(lines, "    ", `${jsonPath}.defaults.codeLigatures`);
      lines.push(`    pub const CODE_LIGATURES: bool = ${defaults.codeLigatures === false ? "false" : "true"};`);
    }
  }
  for (const [key, token] of Object.entries(scale)) {
    if (key.startsWith("$")) continue;
    emitPxConst(lines, "    ", withPxSuffix(`TEXT_${screamingSnake(key)}`), `${jsonPath}.scale.${key}`, token);
  }
  lines.push("}", "");
}

function emitColorSection(lines) {
  lines.push(
    "/// 双模式颜色令牌。",
    "///",
    "/// 不透明令牌为 `0xRRGGBB`；带透明度的令牌为 `0xRRGGBBAA`（同一 `u32`，",
    "/// 逐条在文档注释中标注），避免把 alpha 丢掉或另建第二份色板。",
    "pub mod color {"
  );
  for (const modeName of ["dark", "light"]) {
    const palette = raw.color[modeName];
    const title = modeName === "dark" ? "深色模式（默认）颜色令牌。" : "浅色模式颜色令牌。";
    lines.push(`    /// ${title}`, `    pub mod ${modeName} {`);
    for (const [groupName, group] of Object.entries(palette)) {
      if (groupName.startsWith("$")) continue;
      const prefix = `${screamingSnake(groupName)}_`;
      for (const [key, token] of Object.entries(group)) {
        if (key.startsWith("$") || !token || typeof token !== "object" || !("$value" in token)) continue;
        emitColorConst(lines, "        ", `${prefix}${screamingSnake(key)}`, `color.${modeName}.${groupName}.${key}`, token);
      }
    }
    lines.push("    }", "");
  }
  lines.push("}", "");
}

/** 统一收尾：压缩模块闭合前的多余空行，并保证单个文件末尾换行。 */
function finalizeRustLines(lines) {
  const out = [];
  for (let i = 0; i < lines.length; i += 1) {
    const current = lines[i];
    if (current === "" && lines[i + 1] !== undefined && lines[i + 1].trim() === "}") {
      continue;
    }
    out.push(current);
  }
  while (out.length > 0 && out[out.length - 1] === "") {
    out.pop();
  }
  out.push("");
  return out;
}

function buildRustDocument() {
  const lines = [
    "//! 由 scripts/sync-design-tokens.mjs 生成，禁止手改。",
    "//!",
    "//! 唯一真相源：`design-system/soxu/tokens.json`。本文件与 `design-system/soxu/tokens.css`、",
    "//! `apps/mobile-web/src/tokens.generated.css` 同批编译，并由同一 `--check` 做逐字节零漂移校验。",
    "//!",
    "//! 重新生成：`node scripts/sync-design-tokens.mjs --write`",
    "//! 漂移校验：`node scripts/sync-design-tokens.mjs --check`",
    "//!",
    "//! 命名规则：令牌键 → `SCREAMING_SNAKE_CASE`；尺寸加 `_PX`（`f32`），动效时长加 `_MS`",
    "//! （`u64`），颜色为 `0xRRGGBB`（含 alpha 时 `0xRRGGBBAA`，`u32`）。",
    "",
    "/// 令牌来源的文档元信息。",
    "pub mod meta {",
    `    /// 源令牌 \`meta.name\`。`,
    `    pub const NAME: &str = ${rustStringLiteral(raw.meta.name)};`,    `    /// 源令牌 \`meta.version\`。`,
    `    pub const VERSION: &str = ${rustStringLiteral(raw.meta.version)};`,
    '    /// 源令牌 `meta.defaultMode`。',
    `    pub const DEFAULT_MODE: &str = ${rustStringLiteral(raw.meta.defaultMode)};`,
    "}",
    "",
  ];

  emitTypographySection(lines);
  emitColorSection(lines);

  emitFlatGroup(lines, {
    title: "间距刻度（4px 基准）。",
    moduleName: "spacing",
    jsonPath: "spacing",
    group: raw.spacing,
    prefix: "SPACE_",
    emitToken: (out, indent, name, jsonPath, token) => emitPxConst(out, indent, withPxSuffix(name), jsonPath, token),
  });

  emitFlatGroup(lines, {
    title: "圆角刻度。",
    moduleName: "radius",
    jsonPath: "radius",
    group: raw.radius,
    prefix: "RADIUS_",
    emitToken: (out, indent, name, jsonPath, token) => emitPxConst(out, indent, withPxSuffix(name), jsonPath, token),
  });

  emitFlatGroup(lines, {
    title: "布局栏宽、内容宽度、触控目标与响应式断点（含 400 / 500 / 640 / 1024）。",
    moduleName: "layout",
    jsonPath: "layout",
    group: raw.layout,
    emitToken: (out, indent, name, jsonPath, token) => emitPxConst(out, indent, withPxSuffix(name), jsonPath, token),
  });

  emitFlatGroup(lines, {
    title: "组件几何尺寸（含 44 / 48 触控热区）。",
    moduleName: "sizing",
    jsonPath: "sizing",
    group: raw.sizing,
    emitToken: (out, indent, name, jsonPath, token) => emitPxConst(out, indent, withPxSuffix(name), jsonPath, token),
  });

  emitFlatGroup(lines, {
    title: "层叠顺序。",
    moduleName: "z_index",
    jsonPath: "zIndex",
    group: raw.zIndex,
    emitToken: (out, indent, name, jsonPath, token) => emitNumberConst(out, indent, name, jsonPath, token),
  });

  lines.push("/// 动效令牌：时长（`u64` 毫秒）、缓动曲线与刷新频率上限。", "pub mod motion {");
  for (const [key, token] of Object.entries(raw.motion)) {
    if (key.startsWith("$")) continue;
    const jsonPath = `motion.${key}`;
    const constName = screamingSnake(key);
    const type = token.$type ?? "";
    if (type === "duration") {
      emitMsConst(lines, "    ", `${constName}_MS`, jsonPath, token);
    } else if (type === "cubicBezier") {
      emitCubicBezierConst(lines, "    ", constName, jsonPath, token);
    } else if (typeof token.$value === "number") {
      emitNumberConst(lines, "    ", `${constName}`, jsonPath, token, "f32");
    } else {
      throw new Error(`[tokens] 未支持的动效令牌类型 ${jsonPath}: ${JSON.stringify(token)}`);
    }
  }
  lines.push("}", "");

  lines.push("/// 强调色预设（深/浅双模式各一枚不透明色）。", "pub mod accent_presets {");
  for (const [presetId, preset] of Object.entries(raw.accentPresets)) {
    if (presetId.startsWith("$")) continue;
    const moduleName = presetId.replace(/[^0-9A-Za-z]/g, "_").toLowerCase();
    lines.push(`    /// 预设 \`accentPresets.${presetId}\`：${preset.name}。`, `    pub mod ${moduleName} {`);
    for (const variant of ["dark", "light"]) {
      if (preset[variant] === undefined) continue;
      emitColorConst(
        lines,
        "        ",
        variant.toUpperCase(),
        `accentPresets.${presetId}.${variant}`,
        { $value: preset[variant] }
      );
    }
    lines.push("    }", "");
  }
  lines.push("}", "");

  return finalizeRustLines(lines).join("\n");
}

const expectedCss = buildCssDocument();
const expectedRust = buildRustDocument();
const targets = [
  { path: designCssPath, content: expectedCss },
  { path: mobileCssPath, content: expectedCss },
  { path: rustTokensPath, content: expectedRust },
];

function relativeToRepo(targetPath) {
  return path.relative(repoRoot, targetPath).replace(/\\/g, "/");
}

if (mode === "check") {
  let failed = false;
  for (const target of targets) {
    const name = relativeToRepo(target.path);
    if (!fs.existsSync(target.path)) {
      console.error(`[tokens:check] 缺失生成文件: ${name}`);
      failed = true;
      continue;
    }
    const actual = fs.readFileSync(target.path, "utf8").replace(/\r\n/g, "\n");
    if (actual !== target.content) {
      console.error(
        `[tokens:check] 检测到令牌漂移: ${name}，请运行 node scripts/sync-design-tokens.mjs --write`
      );
      failed = true;
    }
  }
  if (failed) {
    process.exit(1);
  }
  console.log("[tokens:check] 双端 CSS 与 Rust 常量完全同步，无漂移。");
} else {
  for (const target of targets) {
    fs.mkdirSync(path.dirname(target.path), { recursive: true });
    fs.writeFileSync(target.path, target.content, "utf8");
    console.log(`[tokens:write] 已生成 ${relativeToRepo(target.path)}`);
  }
}
