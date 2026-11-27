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
  "composer.addMore": "添加上下文或更多操作",
  "composer.handoff": "移交",
  "composer.handoffChoose": "选择移交代理、模型与思考强度",
  "composer.handoffModelLabel": "指定目标模型",
  "composer.handoffThinkingLabel": "指定思考强度",
  "composer.handoffNoThinking": "该目标代理不支持独立调节思考强度",
  "composer.handoffConfirm": "确认移交配置",
  "composer.handoffClear": "取消移交",
  "composer.permTooltipTitle": "操作权限：{label}",
  "composer.modeSteer": "热纠偏",
  "composer.modeSteerDesc": "在下一工具边界实时注入指令，不中断当前推理",
  "composer.modeQueue": "排队",
  "composer.modeQueueDesc": "加入本轮末尾待发队列，待当前回合结束后自动发出",
  "composer.modeInterrupt": "打断重发",
  "composer.modeInterruptDesc": "立即终止当前正在执行的回合并发送新指令",
  "composer.promoteSteer": "提级为热纠偏",
  "composer.promoteSteerDesc": "立即将排队消息转为热纠偏注入当前回合",
  "composer.cancelQueue": "撤回排队消息",
  "composer.send": "发送指令",
  "composer.stop": "停止输出",

  "stats.title": "会话统计",
  "stats.turnsSteps": "轮次 / 步数",
  "stats.totalTokens": "累计 Token",
  "stats.apiCost": "API 参考费用",
  "stats.avgSpeed": "平均速度",
  "stats.peakSpeed": "峰值速度",
  "stats.recentSpeed": "最近速度",
  "stats.cacheHit": "缓存命中",
  "stats.cacheNote": "缓存观察",
  "stats.more": "更多统计",
  "stats.modelTime": "模型用时",
  "stats.toolTime": "工具用时",
  "stats.ttft": "首 Token 延迟",

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

  "inbox.title": "收件箱分诊台",
  "inbox.pendingCount": "项待你处理",
  "inbox.groupByStatus": "按状态",
  "inbox.groupByProject": "按项目",
  "inbox.filterAll": "全部",
  "inbox.filterNeedsInput": "等你处理",
  "inbox.filterFailed": "出错",
  "inbox.filterAttention": "新完成",
  "inbox.filterRunning": "运行中",
  "inbox.openSession": "去回复",
  "inbox.markAllRead": "全部标为已读",
  "inbox.empty": "当前筛选下暂无待处理事项",

  "sidebar.inbox": "收件箱",
  "sidebar.pinned": "已置顶",
  "sidebar.pinAction": "置顶工作区",
  "sidebar.unpinAction": "取消置顶",

  "inspector.artifact": "产物",
  "inspector.maximize": "全宽审阅模式",
  "inspector.restoreWidth": "恢复右栏宽度",
  "inspector.filterPlaceholder": "按 / 过滤改动文件路径...",
  "inspector.baseUncommitted": "未提交改动",
  "inspector.baseMain": "对比 origin/main",
  "inspector.baseCheckpoint": "对比检查点 #04",

  "diff.addComment": "批注此行",
  "diff.commentPlaceholder": "输入评审意见，回车加入输入舱...",
  "diff.commentSubmit": "加入输入舱",

  "queue.title": "待发指令队列 ({count})",
  "queue.pause": "暂停队列",
  "queue.resume": "恢复队列",
  "queue.paused": "已暂停",
  "queue.edit": "编辑",
  "queue.moveUp": "上移",
  "queue.moveDown": "下移",

  "quote.actionQuote": "引用回复",
  "quote.actionExplain": "旁路解释",
  "quote.explainTitle": "选区旁路解释",
  "quote.explainPlaceholder": "围绕这段内容继续追问...",
  "quote.copyAnswer": "复制回答",
  "quote.convertToQuote": "转为引用回复",

  "turn.branchSession": "分叉会话",
  "artifact.preview": "右栏预览",
  "artifact.approve": "批准执行",
  "artifact.approved": "已批准",
  "focus.title": "实时步骤焦点",

  "mobile.dockExpand": "展开输入舱",
  "mobile.scrollToBottom": "回到底部",
  "mobile.viewSwitch": "切换视图",
  "mobile.navInbox": "收件箱",
  "mobile.navSettings": "设置",
  "mobile.foldDualHint": "双屏分栏模式",

  "appearance.windowChromeTitle": "窗口标题栏风格预览",
  "appearance.windowChromeAuto": "自动跟随系统",
  "appearance.windowChromeMac": "macOS 红绿灯留白",
  "appearance.windowChromeWin": "Windows / Linux 标准",
  "appearance.foldPreviewTitle": "折叠屏 / 双屏视口模拟",
  "appearance.foldPreviewAuto": "自动探测真实硬件段",
  "appearance.foldPreviewDualH": "左右双屏 (折叠展开 / Duo)",
  "appearance.foldPreviewDualV": "上下双段 (桌面半折悬停态)",
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
  "composer.addMore": "新增上下文或更多操作",
  "composer.handoff": "移交",
  "composer.handoffChoose": "選擇移交代理、模型與思考強度",
  "composer.handoffModelLabel": "指定目標模型",
  "composer.handoffThinkingLabel": "指定思考強度",
  "composer.handoffNoThinking": "該目標代理不支援獨立調節思考強度",
  "composer.handoffConfirm": "確認移交設定",
  "composer.handoffClear": "取消移交",
  "composer.permTooltipTitle": "操作權限：{label}",
  "composer.modeSteer": "熱糾偏",
  "composer.modeSteerDesc": "在下一工具邊界即時注入指令，不中斷目前推理",
  "composer.modeQueue": "排隊",
  "composer.modeQueueDesc": "加入本輪末尾待發佇列，待目前回合結束後自動發出",
  "composer.modeInterrupt": "中斷重發",
  "composer.modeInterruptDesc": "立即終止目前正在執行的回合並傳送新指令",
  "composer.promoteSteer": "提級為熱糾偏",
  "composer.promoteSteerDesc": "立即將排隊訊息轉為熱糾偏注入目前回合",
  "composer.cancelQueue": "撤回排隊訊息",
  "composer.send": "傳送指令",
  "composer.stop": "停止輸出",

  "stats.title": "會話統計",
  "stats.turnsSteps": "輪次 / 步數",
  "stats.totalTokens": "累計 Token",
  "stats.apiCost": "API 參考費用",
  "stats.avgSpeed": "平均速度",
  "stats.peakSpeed": "峰值速度",
  "stats.recentSpeed": "最近速度",
  "stats.cacheHit": "快取命中",
  "stats.cacheNote": "快取觀察",
  "stats.more": "更多統計",
  "stats.modelTime": "模型用時",
  "stats.toolTime": "工具用時",
  "stats.ttft": "首 Token 延遲",

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

  "inbox.title": "收件匣分診台",
  "inbox.pendingCount": "項待您處理",
  "inbox.groupByStatus": "依狀態",
  "inbox.groupByProject": "依專案",
  "inbox.filterAll": "全部",
  "inbox.filterNeedsInput": "待您處理",
  "inbox.filterFailed": "出錯",
  "inbox.filterAttention": "新完成",
  "inbox.filterRunning": "執行中",
  "inbox.openSession": "前往回覆",
  "inbox.markAllRead": "全部標為已讀",
  "inbox.empty": "目前篩選條件下無待處理事項",

  "sidebar.inbox": "收件匣",
  "sidebar.pinned": "已釘選",
  "sidebar.pinAction": "釘選工作區",
  "sidebar.unpinAction": "取消釘選",

  "inspector.artifact": "產物",
  "inspector.maximize": "全寬審閱模式",
  "inspector.restoreWidth": "還原右欄寬度",
  "inspector.filterPlaceholder": "依 / 過濾異動檔案路徑...",
  "inspector.baseUncommitted": "未提交異動",
  "inspector.baseMain": "對比 origin/main",
  "inspector.baseCheckpoint": "對比檢查點 #04",

  "diff.addComment": "批註此行",
  "diff.commentPlaceholder": "輸入評審意見，按 Enter 加入輸入艙...",
  "diff.commentSubmit": "加入輸入艙",

  "queue.title": "待發指令佇列 ({count})",
  "queue.pause": "暫停佇列",
  "queue.resume": "恢復佇列",
  "queue.paused": "已暫停",
  "queue.edit": "編輯",
  "queue.moveUp": "上移",
  "queue.moveDown": "下移",

  "quote.actionQuote": "引用回覆",
  "quote.actionExplain": "旁路解釋",
  "quote.explainTitle": "選區旁路解釋",
  "quote.explainPlaceholder": "針對這段內容繼續追問...",
  "quote.copyAnswer": "複製回答",
  "quote.convertToQuote": "轉為引用回覆",

  "turn.branchSession": "分叉會話",
  "artifact.preview": "右欄預覽",
  "artifact.approve": "批准執行",
  "artifact.approved": "已批准",
  "focus.title": "即時步驟焦點",

  "mobile.dockExpand": "展開輸入艙",
  "mobile.scrollToBottom": "捲動到底部",
  "mobile.viewSwitch": "切換檢視",
  "mobile.navInbox": "收件匣",
  "mobile.navSettings": "設定",
  "mobile.foldDualHint": "雙螢幕分欄模式",

  "appearance.windowChromeTitle": "視窗標題列風格預覽",
  "appearance.windowChromeAuto": "自動跟隨系統",
  "appearance.windowChromeMac": "macOS 紅綠燈留白",
  "appearance.windowChromeWin": "Windows / Linux 標準",
  "appearance.foldPreviewTitle": "摺疊屏 / 雙螢幕視口模擬",
  "appearance.foldPreviewAuto": "自動偵測真實硬體區段",
  "appearance.foldPreviewDualH": "左右雙螢幕 (摺疊展開 / Duo)",
  "appearance.foldPreviewDualV": "上下雙段 (桌面半摺懸停態)",
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
  "composer.addMore": "Add context or more actions",
  "composer.handoff": "Handoff",
  "composer.handoffChoose": "Select handoff agent, model and thinking",
  "composer.handoffModelLabel": "Target Model",
  "composer.handoffThinkingLabel": "Thinking Level",
  "composer.handoffNoThinking": "Target agent does not expose thinking levels",
  "composer.handoffConfirm": "Apply Handoff",
  "composer.handoffClear": "Clear Handoff",
  "composer.permTooltipTitle": "Permission: {label}",
  "composer.modeSteer": "Steer",
  "composer.modeSteerDesc": "Inject guidance at the next tool boundary without stopping",
  "composer.modeQueue": "Queue",
  "composer.modeQueueDesc": "Queue prompt to send automatically after current turn",
  "composer.modeInterrupt": "Interrupt",
  "composer.modeInterruptDesc": "Abort active turn immediately and resend new prompt",
  "composer.promoteSteer": "Promote to live steer",
  "composer.promoteSteerDesc": "Inject queued message into active turn immediately",
  "composer.cancelQueue": "Cancel queued prompt",
  "composer.send": "Send prompt",
  "composer.stop": "Stop output",

  "stats.title": "Session Stats",
  "stats.turnsSteps": "Turns / Steps",
  "stats.totalTokens": "Cumulative Tokens",
  "stats.apiCost": "Est. API Cost",
  "stats.avgSpeed": "Avg Speed",
  "stats.peakSpeed": "Peak Speed",
  "stats.recentSpeed": "Recent Speed",
  "stats.cacheHit": "Cache Hit",
  "stats.cacheNote": "Cache Observe",
  "stats.more": "More stats",
  "stats.modelTime": "Model Time",
  "stats.toolTime": "Tool Time",
  "stats.ttft": "First Token Latency",

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

  "inbox.title": "Inbox Triage",
  "inbox.pendingCount": "needs your attention",
  "inbox.groupByStatus": "By Status",
  "inbox.groupByProject": "By Project",
  "inbox.filterAll": "All",
  "inbox.filterNeedsInput": "Needs Input",
  "inbox.filterFailed": "Failed",
  "inbox.filterAttention": "Ready",
  "inbox.filterRunning": "Running",
  "inbox.openSession": "Reply",
  "inbox.markAllRead": "Mark all read",
  "inbox.empty": "Nothing waiting under current filter",

  "sidebar.inbox": "Inbox",
  "sidebar.pinned": "Pinned",
  "sidebar.pinAction": "Pin workspace",
  "sidebar.unpinAction": "Unpin",

  "inspector.artifact": "Artifact",
  "inspector.maximize": "Maximize inspector",
  "inspector.restoreWidth": "Restore inspector width",
  "inspector.filterPlaceholder": "Filter changed files (/)...",
  "inspector.baseUncommitted": "Uncommitted",
  "inspector.baseMain": "vs origin/main",
  "inspector.baseCheckpoint": "vs Checkpoint #04",

  "diff.addComment": "Comment on line",
  "diff.commentPlaceholder": "Add inline review note, Enter to attach...",
  "diff.commentSubmit": "Attach note",

  "queue.title": "Queued Prompts ({count})",
  "queue.pause": "Pause queue",
  "queue.resume": "Resume queue",
  "queue.paused": "Paused",
  "queue.edit": "Edit",
  "queue.moveUp": "Move up",
  "queue.moveDown": "Move down",

  "quote.actionQuote": "Quote reply",
  "quote.actionExplain": "Explain",
  "quote.explainTitle": "Side Explain",
  "quote.explainPlaceholder": "Ask a follow-up about this selection...",
  "quote.copyAnswer": "Copy answer",
  "quote.convertToQuote": "Quote in composer",

  "turn.branchSession": "Branch",
  "artifact.preview": "Preview",
  "artifact.approve": "Approve plan",
  "artifact.approved": "Approved",
  "focus.title": "Live Focus Steps",

  "mobile.dockExpand": "Expand composer",
  "mobile.scrollToBottom": "Scroll to bottom",
  "mobile.viewSwitch": "Switch view",
  "mobile.navInbox": "Inbox",
  "mobile.navSettings": "Settings",
  "mobile.foldDualHint": "Dual-pane fold mode",

  "appearance.windowChromeTitle": "Window Chrome Preview",
  "appearance.windowChromeAuto": "Match OS",
  "appearance.windowChromeMac": "macOS Traffic Lights",
  "appearance.windowChromeWin": "Windows / Linux Standard",
  "appearance.foldPreviewTitle": "Foldable / Dual-Screen Simulation",
  "appearance.foldPreviewAuto": "Auto (Hardware Segments)",
  "appearance.foldPreviewDualH": "Dual Horizontal (Fold / Duo)",
  "appearance.foldPreviewDualV": "Dual Vertical (Flex Tabletop)",
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
