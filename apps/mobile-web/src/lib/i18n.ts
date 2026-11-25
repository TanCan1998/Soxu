import { createContext, createElement, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/**
 * Soxu 可扩展国际化注册中心：
 * - `zh-CN` 作为类型推导与兜底回退基准词典；
 * - 内置 `zh-CN`（简体中文）、`zh-TW`（繁體中文）与 `en-US`（English）；
 * - 暴露 `registerLocalePack` 允许外部模块、插件或按需懒加载语言包动态注册与覆盖词条，无需改动调用方组件。
 */

export const ZH_CN_MESSAGES = {
  "brand.name": "Soxu",

  "header.toggleLeft": "折叠或展开左侧栏",
  "header.toggleRight": "折叠或展开右侧栏",
  "header.openLeftDrawer": "打开工作区导航抽屉",
  "header.openRightDrawer": "打开变更与脚本抽屉",
  "header.quickChat": "快速对话",
  "header.idleWorkbench": "空闲工作台",
  "header.searchPlaceholder": "搜索会话、文件或系统指令...",
  "header.searchCompact": "搜索会话或指令...",
  "header.moreTools": "更多工作台工具（悬停自动展开）",
  "header.settings": "系统设置",
  "header.pair": "扫码配对",

  "sidebar.title": "快速对话与项目工作树",
  "sidebar.quickChats": "快速对话",
  "sidebar.quickChatsCount": "快速对话 ({count})",
  "sidebar.newQuickChat": "新建无项目自由对话",
  "sidebar.projectsCount": "项目工作区 ({count})",
  "sidebar.archivedCount": "已归档对话 ({count})",
  "sidebar.noArchived": "暂无已归档对话",
  "sidebar.restore": "恢复",
  "sidebar.archiveAction": "归档对话",
  "sidebar.worktree": "工作树",
  "sidebar.localRoot": "主目录",
  "sidebar.importHistory": "导入历史",
  "sidebar.schedules": "定时任务",
  "sidebar.gatewayManaged": "协同网关",
  "sidebar.gatewayDirect": "原生直连",

  "status.needsInput": "待批",
  "status.failed": "失败",
  "status.running": "运行中",
  "status.attention": "新完成",
  "status.done": "空闲",

  "tabs.closeTab": "关闭标签页",
  "tabs.archiveTab": "归档标签页",
  "tabs.emptyTitle": "所有会话标签均已关闭",
  "tabs.emptyDesc": "你可以随时发起无项目自由对话，或在左侧栏点选已有项目与工作树继续推进任务。",
  "tabs.newWorkspace": "打开项目工作树",
  "tabs.newQuick": "新建无项目对话",
  "tabs.quickModeBadge": "轻量模式",
  "tabs.quickBanner": "无项目快速对话 · 不绑定本地工作树，随时可自由交流或构思方案",

  "inspector.title": "上下文与代码审阅栏",
  "inspector.diff": "变更对比",
  "inspector.files": "文件树",
  "inspector.scripts": "脚本",
  "inspector.terminal": "终端",
  "inspector.diffUnified": "单栏",
  "inspector.diffSplit": "双栏",
  "inspector.commitChanges": "提交变更",
  "inspector.baseBranchLabel": "对比基准 {branch}",
  "inspector.emptyTitle": "当前处于无项目自由对话模式，未挂载工作树目录",

  "composer.placeholderQuick": "输入任意问题开启无项目自由对话，回车直接发送...",
  "composer.placeholderWork": "输入指令、按需要选择运行中纠偏模式或移交代理...",
  "composer.handoff": "移交",
  "composer.modeSteer": "热纠偏",
  "composer.modeQueue": "排队",
  "composer.modeInterrupt": "打断重发",
  "composer.send": "发送指令",
  "composer.stop": "停止输出",

  "footer.daemon": "本地守护服务 {addr} (游标 #{seq})",
  "footer.devices": "{count} 台设备在线",

  "mobile.navSession": "会话",
  "mobile.navWorkspaces": "工作区",
  "mobile.navApprovals": "审批",
  "mobile.navScripts": "脚本",
  "mobile.daemonOnline": "守护服务在线",
  "mobile.remoteClient": "遥控客户端",
  "mobile.cursorPrefix": "游标 #{seq}",

  "settings.title": "系统设置",
  "settings.description": "细化调整界面外观、多语言与主题色、开源双字体排版、代理调度、工作树及移动遥控",
  "settings.section.appearance": "外观与布局",
  "settings.section.fonts": "字体与代码排版",
  "settings.section.magpie": "代理与路由器",
  "settings.section.git": "工作区与版本",
  "settings.section.security": "安全与审批红线",
  "settings.section.mobile": "移动遥控与配对",
  "settings.section.about": "快捷键与关于",

  "appearance.heading": "外观主题、语言与工作区布局",
  "appearance.subheading": "深色模式采用零色偏纯黑灰阶层级；支持即时切换界面语言与全局强调色，并带平滑过渡动效。",
  "appearance.themeDarkTitle": "纯黑深色",
  "appearance.themeDarkDesc": "纯净 #000000 画布",
  "appearance.themeLightTitle": "纯净浅色",
  "appearance.themeLightDesc": "高对比 #FFFFFF 日光模式",
  "appearance.themeSystemTitle": "跟随系统",
  "appearance.themeSystemDesc": "同步操作系统明暗设置",
  "appearance.languageLabel": "界面语言",
  "appearance.languageHint": "支持通过语言包注册表扩展更多语种",
  "appearance.accentHeading": "主题强调色",
  "appearance.accentSubheading": "用于品牌标识高亮、焦点态提示、活跃状态点与选中衬底（支持自定义色值）",
  "appearance.accentPresetEmerald": "翠玉绿",
  "appearance.accentPresetOcean": "极海蓝",
  "appearance.accentPresetViolet": "星云紫",
  "appearance.accentPresetAmber": "琥珀橙",
  "appearance.accentPresetRose": "绯霞红",
  "appearance.accentPresetCyan": "晴空青",
  "appearance.accentCustom": "自定义",
  "appearance.accentCustomInput": "自定义色值",
  "appearance.accentPreviewTag": "当前强调色",
  "appearance.lineHeight": "正文排版行距",
  "appearance.lineHeightCompact": "紧凑阅读 (1.38 倍)",
  "appearance.lineHeightNormal": "标准平衡 (1.50 倍)",
  "appearance.lineHeightRelaxed": "舒展呼吸 (1.68 倍)",
  "appearance.timestampFormat": "消息时间戳格式",
  "appearance.timestampRelative": "相对时间（例如 2 分钟前）",
  "appearance.timestampAbsolute": "精确时刻（例如 07:45:12）",
  "appearance.densityTitle": "高密度工作区列表",
  "appearance.densityDesc": "项目与工作树行采用 32 像素紧凑高度（关闭切换为 38 像素舒适高度）",
  "appearance.reduceMotionTitle": "减弱界面动画效果",
  "appearance.reduceMotionDesc": "关闭主题径向渐变与折叠过渡，瞬时响应所有交互状态切换",
  "appearance.sidebarWidths": "当前侧栏宽度：左栏 {left}px，右栏 {right}px",
  "appearance.resetWidths": "重置栏宽",
  "appearance.resetWidthsToast": "已将左右侧栏宽度重置为默认值",
  "appearance.localeChangedToast": "界面语言已切换为 {label}",
  "appearance.accentChangedToast": "主题强调色已应用：{label}",

  "about.heading": "快捷键与关于 Soxu",
  "onboarding.cta": "开始使用 Soxu",
  "onboarding.welcomeToast": "欢迎开始使用 Soxu",
} as const;

export type I18nKey = keyof typeof ZH_CN_MESSAGES;
export type BuiltinLocaleId = "zh-CN" | "zh-TW" | "en-US";
export type LocaleId = BuiltinLocaleId | (string & {});

export interface LocalePack {
  id: string;
  label: string;
  nativeNote?: string;
  dir?: "ltr" | "rtl";
  messages: Partial<Record<I18nKey, string>>;
}

const ZH_TW_MESSAGES: Partial<Record<I18nKey, string>> = {
  "header.toggleLeft": "摺疊或展開左側欄",
  "header.toggleRight": "摺疊或展開右側欄",
  "header.openLeftDrawer": "開啟工作區導航抽屜",
  "header.openRightDrawer": "開啟變更與腳本抽屜",
  "header.quickChat": "快速對話",
  "header.idleWorkbench": "閒置工作台",
  "header.searchPlaceholder": "搜尋工作階段、檔案或系統指令...",
  "header.searchCompact": "搜尋工作階段或指令...",
  "header.moreTools": "更多工作台工具（懸停自動展開）",
  "header.settings": "系統設定",
  "header.pair": "掃碼配對",

  "sidebar.title": "快速對話與專案工作樹",
  "sidebar.quickChats": "快速對話",
  "sidebar.quickChatsCount": "快速對話 ({count})",
  "sidebar.newQuickChat": "新建無專案自由對話",
  "sidebar.projectsCount": "專案工作區 ({count})",
  "sidebar.archivedCount": "已封存對話 ({count})",
  "sidebar.noArchived": "暫無已封存對話",
  "sidebar.restore": "還原",
  "sidebar.archiveAction": "封存對話",
  "sidebar.worktree": "工作樹",
  "sidebar.localRoot": "主目錄",
  "sidebar.importHistory": "匯入歷史",
  "sidebar.schedules": "排程任務",
  "sidebar.gatewayManaged": "協同閘道",
  "sidebar.gatewayDirect": "原生直連",

  "status.needsInput": "待批",
  "status.failed": "失敗",
  "status.running": "執行中",
  "status.attention": "新完成",
  "status.done": "閒置",

  "tabs.closeTab": "關閉分頁",
  "tabs.archiveTab": "封存分頁",
  "tabs.emptyTitle": "所有工作階段分頁均已關閉",
  "tabs.emptyDesc": "您可以隨時發起無專案自由對話，或在左側欄點選現有專案與工作樹繼續任務。",
  "tabs.newWorkspace": "開啟專案工作樹",
  "tabs.newQuick": "新建無專案對話",
  "tabs.quickModeBadge": "輕量模式",
  "tabs.quickBanner": "無專案快速對話 · 不綁定本地工作樹，隨時自由討論與構思方案",

  "inspector.title": "上下文與程式碼審閱欄",
  "inspector.diff": "變更對比",
  "inspector.files": "檔案樹",
  "inspector.scripts": "腳本",
  "inspector.terminal": "終端機",
  "inspector.diffUnified": "單欄",
  "inspector.diffSplit": "雙欄",
  "inspector.commitChanges": "提交變更",
  "inspector.baseBranchLabel": "對比基準 {branch}",
  "inspector.emptyTitle": "目前為無專案自由對話模式，未掛載工作樹目錄",

  "composer.placeholderQuick": "輸入任意問題開啟無專案自由對話，按 Enter 傳送...",
  "composer.placeholderWork": "輸入指令、依需要選擇執行中糾偏模式或移交代理...",
  "composer.handoff": "移交",
  "composer.modeSteer": "熱糾偏",
  "composer.modeQueue": "排隊",
  "composer.modeInterrupt": "中斷重發",
  "composer.send": "傳送指令",
  "composer.stop": "停止輸出",

  "footer.daemon": "本地守護服務 {addr} (游標 #{seq})",
  "footer.devices": "{count} 台裝置在線",

  "mobile.navSession": "對話",
  "mobile.navWorkspaces": "工作區",
  "mobile.navApprovals": "審批",
  "mobile.navScripts": "腳本",
  "mobile.daemonOnline": "守護服務在線",
  "mobile.remoteClient": "遙控用戶端",
  "mobile.cursorPrefix": "游標 #{seq}",

  "settings.title": "系統設定",
  "settings.description": "細緻調整介面外觀、多語言與主題色、開源雙字體排版、代理調度、工作樹與行動遙控",
  "settings.section.appearance": "外觀與版面",
  "settings.section.fonts": "字體與程式碼排版",
  "settings.section.magpie": "代理與路由器",
  "settings.section.git": "工作區與版本",
  "settings.section.security": "安全與審批紅線",
  "settings.section.mobile": "行動遙控與配對",
  "settings.section.about": "快捷鍵與關於",

  "appearance.heading": "外觀主題、語言與工作區版面",
  "appearance.subheading": "深色模式採用零色偏純黑灰階層級；支援即時切換介面語言與全域強調色。",
  "appearance.themeDarkTitle": "純黑深色",
  "appearance.themeDarkDesc": "純淨 #000000 畫布",
  "appearance.themeLightTitle": "純淨淺色",
  "appearance.themeLightDesc": "高對比 #FFFFFF 日光模式",
  "appearance.themeSystemTitle": "跟隨系統",
  "appearance.themeSystemDesc": "同步作業系統明暗設定",
  "appearance.languageLabel": "介面語言",
  "appearance.languageHint": "支援透過語言包註冊表動態擴充更多語系",
  "appearance.accentHeading": "主題強調色",
  "appearance.accentSubheading": "用於品牌強調、焦點狀態、活躍狀態指示與選取襯底（支援自訂色碼）",
  "appearance.accentPresetEmerald": "翠玉綠",
  "appearance.accentPresetOcean": "極海藍",
  "appearance.accentPresetViolet": "星雲紫",
  "appearance.accentPresetAmber": "琥珀澄",
  "appearance.accentPresetRose": "緋霞紅",
  "appearance.accentPresetCyan": "晴空青",
  "appearance.accentCustom": "自訂",
  "appearance.accentCustomInput": "自訂色碼",
  "appearance.accentPreviewTag": "目前強調色",
  "appearance.lineHeight": "正文排版行距",
  "appearance.lineHeightCompact": "緊湊閱讀 (1.38 倍)",
  "appearance.lineHeightNormal": "標準平衡 (1.50 倍)",
  "appearance.lineHeightRelaxed": "舒展呼吸 (1.68 倍)",
  "appearance.timestampFormat": "訊息時間戳記格式",
  "appearance.timestampRelative": "相對時間（例如 2 分鐘前）",
  "appearance.timestampAbsolute": "精確時刻（例如 07:45:12）",
  "appearance.densityTitle": "高密度工作區列表",
  "appearance.densityDesc": "專案與工作樹列採用 32px 緊湊高度（關閉則為 38px 舒適高度）",
  "appearance.reduceMotionTitle": "減弱介面動畫效果",
  "appearance.reduceMotionDesc": "關閉主題徑向漸變與摺疊過渡動畫",
  "appearance.sidebarWidths": "目前側欄寬度：左欄 {left}px，右欄 {right}px",
  "appearance.resetWidths": "重設欄寬",
  "appearance.resetWidthsToast": "已將左右側欄寬度重設為預設值",
  "appearance.localeChangedToast": "介面語言已切換為 {label}",
  "appearance.accentChangedToast": "已套用主題強調色：{label}",

  "about.heading": "快捷鍵與關於 Soxu",
  "onboarding.cta": "開始使用 Soxu",
  "onboarding.welcomeToast": "歡迎開始使用 Soxu",
};

const EN_US_MESSAGES: Partial<Record<I18nKey, string>> = {
  "header.toggleLeft": "Toggle left sidebar",
  "header.toggleRight": "Toggle right inspector",
  "header.openLeftDrawer": "Open workspace drawer",
  "header.openRightDrawer": "Open diff and scripts drawer",
  "header.quickChat": "Quick Chat",
  "header.idleWorkbench": "Idle Workbench",
  "header.searchPlaceholder": "Search sessions, files, or commands...",
  "header.searchCompact": "Search sessions or commands...",
  "header.moreTools": "More workbench tools (hover to expand)",
  "header.settings": "Settings",
  "header.pair": "Scan & Pair",

  "sidebar.title": "Quick Chats & Project Worktrees",
  "sidebar.quickChats": "Quick Chats",
  "sidebar.quickChatsCount": "Quick Chats ({count})",
  "sidebar.newQuickChat": "New projectless quick chat",
  "sidebar.projectsCount": "Workspaces ({count})",
  "sidebar.archivedCount": "Archived ({count})",
  "sidebar.noArchived": "No archived conversations",
  "sidebar.restore": "Restore",
  "sidebar.archiveAction": "Archive",
  "sidebar.worktree": "worktree",
  "sidebar.localRoot": "main",
  "sidebar.importHistory": "History",
  "sidebar.schedules": "Schedules",
  "sidebar.gatewayManaged": "Managed Gateway",
  "sidebar.gatewayDirect": "Direct CLI",

  "status.needsInput": "Approval",
  "status.failed": "Failed",
  "status.running": "Running",
  "status.attention": "Ready",
  "status.done": "Idle",

  "tabs.closeTab": "Close tab",
  "tabs.archiveTab": "Archive session",
  "tabs.emptyTitle": "All session tabs closed",
  "tabs.emptyDesc": "Start a projectless quick chat anytime or pick a repository worktree from the sidebar.",
  "tabs.newWorkspace": "Open Worktree",
  "tabs.newQuick": "New Quick Chat",
  "tabs.quickModeBadge": "Lightweight",
  "tabs.quickBanner": "Projectless Quick Chat · No local worktree attached",

  "inspector.title": "Context & Diff Inspector",
  "inspector.diff": "Diff",
  "inspector.files": "Files",
  "inspector.scripts": "Scripts",
  "inspector.terminal": "Terminal",
  "inspector.diffUnified": "Unified",
  "inspector.diffSplit": "Split",
  "inspector.commitChanges": "Commit",
  "inspector.baseBranchLabel": "Base {branch}",
  "inspector.emptyTitle": "Projectless quick chat active; no git worktree mounted",

  "composer.placeholderQuick": "Ask anything in projectless quick chat, press Enter to send...",
  "composer.placeholderWork": "Send prompt, steer active turns, or @handoff to another agent...",
  "composer.handoff": "Handoff",
  "composer.modeSteer": "Steer",
  "composer.modeQueue": "Queue",
  "composer.modeInterrupt": "Interrupt",
  "composer.send": "Send prompt",
  "composer.stop": "Stop output",

  "footer.daemon": "Daemon {addr} (seq #{seq})",
  "footer.devices": "{count} devices online",

  "mobile.navSession": "Chat",
  "mobile.navWorkspaces": "Workspaces",
  "mobile.navApprovals": "Approvals",
  "mobile.navScripts": "Scripts",
  "mobile.daemonOnline": "Daemon Online",
  "mobile.remoteClient": "Remote Client",
  "mobile.cursorPrefix": "seq #{seq}",

  "settings.title": "Settings",
  "settings.description": "Configure appearance, language, accent palette, open-source fonts, agent routing, git worktrees, and remote pairing",
  "settings.section.appearance": "Appearance & Layout",
  "settings.section.fonts": "Fonts & Code Typography",
  "settings.section.magpie": "Agents & Router",
  "settings.section.git": "Workspaces & Git",
  "settings.section.security": "Security & Guardrails",
  "settings.section.mobile": "Remote & Pairing",
  "settings.section.about": "Shortcuts & About",

  "appearance.heading": "Theme, Language & Layout",
  "appearance.subheading": "Pure Black neutral canvas in dark mode; switch interface language and accent palette instantaneously.",
  "appearance.themeDarkTitle": "Pure Black",
  "appearance.themeDarkDesc": "True #000000 canvas",
  "appearance.themeLightTitle": "Clean Light",
  "appearance.themeLightDesc": "High-contrast #FFFFFF mode",
  "appearance.themeSystemTitle": "System",
  "appearance.themeSystemDesc": "Match OS color scheme",
  "appearance.languageLabel": "Interface Language",
  "appearance.languageHint": "Extensible runtime pack registry via registerLocalePack",
  "appearance.accentHeading": "Theme Accent Color",
  "appearance.accentSubheading": "Used for active highlights, focus cues, status emphasis, and text selection (custom hex supported)",
  "appearance.accentPresetEmerald": "Emerald",
  "appearance.accentPresetOcean": "Ocean Blue",
  "appearance.accentPresetViolet": "Violet",
  "appearance.accentPresetAmber": "Amber",
  "appearance.accentPresetRose": "Rose",
  "appearance.accentPresetCyan": "Cyan",
  "appearance.accentCustom": "Custom",
  "appearance.accentCustomInput": "Custom Hex",
  "appearance.accentPreviewTag": "Active Accent",
  "appearance.lineHeight": "UI Line Height",
  "appearance.lineHeightCompact": "Compact (1.38x)",
  "appearance.lineHeightNormal": "Balanced (1.50x)",
  "appearance.lineHeightRelaxed": "Relaxed (1.68x)",
  "appearance.timestampFormat": "Timestamp Format",
  "appearance.timestampRelative": "Relative (e.g. 2m ago)",
  "appearance.timestampAbsolute": "Absolute (e.g. 07:45:12)",
  "appearance.densityTitle": "Dense Workspace Rows",
  "appearance.densityDesc": "Use 32px compact height for trees and session lists (38px when disabled)",
  "appearance.reduceMotionTitle": "Reduce Motion",
  "appearance.reduceMotionDesc": "Disable radial theme transitions and accordion motion",
  "appearance.sidebarWidths": "Panel widths: Left {left}px, Right {right}px",
  "appearance.resetWidths": "Reset Widths",
  "appearance.resetWidthsToast": "Reset left and right panel widths to defaults",
  "appearance.localeChangedToast": "Language switched to {label}",
  "appearance.accentChangedToast": "Accent color applied: {label}",

  "about.heading": "Shortcuts & About Soxu",
  "onboarding.cta": "Start Using Soxu",
  "onboarding.welcomeToast": "Welcome to Soxu",
};

const localeRegistry = new Map<string, LocalePack>([
  ["zh-CN", { id: "zh-CN", label: "简体中文", nativeNote: "默认基准", dir: "ltr", messages: ZH_CN_MESSAGES }],
  ["zh-TW", { id: "zh-TW", label: "繁體中文", nativeNote: "Traditional Chinese", dir: "ltr", messages: ZH_TW_MESSAGES }],
  ["en-US", { id: "en-US", label: "English", nativeNote: "United States", dir: "ltr", messages: EN_US_MESSAGES }],
]);

const registryListeners = new Set<() => void>();

/**
 * 动态注册或增量合并语言包；未来新增日文、韩文或社区语言时无需修改核心组件逻辑。
 */
export function registerLocalePack(pack: LocalePack): void {
  const existing = localeRegistry.get(pack.id);
  localeRegistry.set(pack.id, {
    id: pack.id,
    label: pack.label,
    nativeNote: pack.nativeNote ?? existing?.nativeNote,
    dir: pack.dir ?? existing?.dir ?? "ltr",
    messages: { ...(existing?.messages ?? {}), ...pack.messages },
  });
  registryListeners.forEach((cb) => cb());
}

export function listLocalePacks(): LocalePack[] {
  return Array.from(localeRegistry.values());
}

export function getLocalePack(locale: string): LocalePack {
  return localeRegistry.get(locale) ?? localeRegistry.get("zh-CN")!;
}

export function translate(locale: string, key: I18nKey, params?: Record<string, string | number>): string {
  const pack = getLocalePack(locale);
  const template = pack.messages[key] ?? ZH_CN_MESSAGES[key];
  if (!params) return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (_, token) =>
    params[token] !== undefined ? String(params[token]) : `{${token}}`,
  );
}

interface I18nContextValue {
  locale: string;
  dir: "ltr" | "rtl";
  locales: LocalePack[];
  t: (key: I18nKey, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue>({
  locale: "zh-CN",
  dir: "ltr",
  locales: listLocalePacks(),
  t: (key, params) => translate("zh-CN", key, params),
});

export function I18nProvider({ locale, children }: { locale: string; children: ReactNode }) {
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    const bump = () => setRevision((r) => r + 1);
    registryListeners.add(bump);
    return () => {
      registryListeners.delete(bump);
    };
  }, []);

  const value = useMemo<I18nContextValue>(() => {
    void revision;
    const pack = getLocalePack(locale);
    return {
      locale: pack.id,
      dir: pack.dir ?? "ltr",
      locales: listLocalePacks(),
      t: (key, params) => translate(pack.id, key, params),
    };
  }, [locale, revision]);

  return createElement(I18nContext.Provider, { value }, children);
}

export function useI18n(): I18nContextValue {
  return useContext(I18nContext);
}
