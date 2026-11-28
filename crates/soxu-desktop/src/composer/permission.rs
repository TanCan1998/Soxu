//! 输入舱常驻「纯图标执行权限」按钮的 6 档状态机（纯逻辑）。
//!
//! 规范来源：
//! - `pages/02-composer-and-handoff.md` §3.2：权限档位**绝不折叠进更多菜单**，
//!   宽版用「图标 + 中文标签」，超窄屏切为 32×32 纯图标钮，`Alt+P` 循环；
//! - `pages/07-settings-fonts-and-motion.md` §2.2 分区 5：默认权限档位属于安全与审批红线；
//! - `MASTER.md` §1.2：语义色必须配对图标或文字标签，严禁仅靠颜色区分状态。
//!
//! 语义枚举与色调复用 [`soxu_protocol`] 既有契约（6 档，kebab-case 序列化），
//! 本模块只补中文标签、图标字形与「按 Agent 能力循环」的状态机，不新增协议类型。
//!
//! 图标说明：GPUI 侧矢量化 Lucide 图标需引入资源包（M5 后续统一接线），本阶段使用
//! **非 Emoji 的几何字形**作为确定性占位图标，并以中文 `aria` 文案补偿无障碍语义。

use soxu_protocol::{PermissionLevel, PermissionTone};

/// 6 档权限的循环与呈现顺序（从最保守到最放行）。
pub const PERMISSION_ORDER: [PermissionLevel; 6] = [
    PermissionLevel::Plan,
    PermissionLevel::ReadOnly,
    PermissionLevel::Ask,
    PermissionLevel::AutoEdit,
    PermissionLevel::FullAccess,
    PermissionLevel::Turbo,
];

/// 单档权限的呈现元数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionMeta {
    /// 协议枚举。
    pub level: PermissionLevel,
    /// 中文短标签（宽版按钮文案与浮层标题）。
    pub label: &'static str,
    /// 中文完整说明（`Alt+P` 循环提示与悬浮说明）。
    pub description: &'static str,
    /// 语义色调（决定图标与描边颜色）。
    pub tone: PermissionTone,
    /// 非 Emoji 的几何占位图标字形。
    pub glyph: &'static str,
}

/// 6 档权限元数据表（顺序同 [`PERMISSION_ORDER`]）。
///
/// 文案沿用移动端已实现的权限语义（`apps/mobile-web/src/lib/agent-capabilities.ts`），
/// 保证双端对同一档位说同一句话。
pub const PERMISSION_CATALOG: [PermissionMeta; 6] = [
    PermissionMeta {
        level: PermissionLevel::Plan,
        label: "只读规划",
        description: "只读取和制定方案，不执行磁盘写入",
        tone: PermissionTone::Neutral,
        glyph: "◇",
    },
    PermissionMeta {
        level: PermissionLevel::ReadOnly,
        label: "只读分析",
        description: "禁止写入任何工作区文件或执行改动命令",
        tone: PermissionTone::Neutral,
        glyph: "◌",
    },
    PermissionMeta {
        level: PermissionLevel::Ask,
        label: "询问确认",
        description: "文件写入或命令执行前请求人工确认",
        tone: PermissionTone::Accent,
        glyph: "◐",
    },
    PermissionMeta {
        level: PermissionLevel::AutoEdit,
        label: "自动编辑",
        description: "工作区内修改自动写入，危险命令仍确认",
        tone: PermissionTone::Warning,
        glyph: "◑",
    },
    PermissionMeta {
        level: PermissionLevel::FullAccess,
        label: "完全授权",
        description: "当前工作树全权自主运行（受红线保护）",
        tone: PermissionTone::Danger,
        glyph: "●",
    },
    PermissionMeta {
        level: PermissionLevel::Turbo,
        label: "自主运行",
        description: "隔离工作树内自动推进，仅高危拦截",
        tone: PermissionTone::Danger,
        glyph: "◆",
    },
];

/// 取某档权限的元数据（`PERMISSION_ORDER` 覆盖全部枚举变体，`get` 不会落空）。
pub fn meta(level: PermissionLevel) -> &'static PermissionMeta {
    PERMISSION_CATALOG
        .iter()
        .find(|m| m.level == level)
        .unwrap_or(&PERMISSION_CATALOG[2])
}

/// 循环方向（`Alt+P` 正向、`Alt+Shift+P` 反向）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleDirection {
    /// 更放行方向（Plan → … → Turbo → Plan）。
    Forward,
    /// 更保守方向。
    Backward,
}

/// 权限档位切换裁决结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionSelection {
    /// 目标档位被当前 Agent 支持，已生效。
    Applied(PermissionLevel),
    /// 目标档位不受支持，已回退到安全档位（对齐 `lost-controls` 回退语义）。
    Fallback {
        /// 用户请求的档位。
        requested: PermissionLevel,
        /// 实际生效的档位。
        applied: PermissionLevel,
    },
    /// 目标与当前一致，状态不变。
    Unchanged(PermissionLevel),
}

/// 6 档权限状态机：持有当前档位与该 Agent 支持的档位集合。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionState {
    current: PermissionLevel,
    available: Vec<PermissionLevel>,
}

impl PermissionState {
    /// 以「全部 6 档可用」和协议默认档位（`Ask`）创建。
    pub fn new() -> Self {
        Self {
            current: PermissionLevel::default(),
            available: PERMISSION_ORDER.to_vec(),
        }
    }

    /// 按某 Agent 的能力矩阵创建：保持传入顺序、去重，空集回退到 `Ask`。
    ///
    /// 若协议默认档位不在支持集合内，则取集合首项（最保守档位）。
    pub fn for_agent(available: impl IntoIterator<Item = PermissionLevel>) -> Self {
        let mut deduped: Vec<PermissionLevel> = Vec::new();
        for level in available {
            if !deduped.contains(&level) {
                deduped.push(level);
            }
        }
        if deduped.is_empty() {
            deduped.push(PermissionLevel::Ask);
        }
        let current = if deduped.contains(&PermissionLevel::default()) {
            PermissionLevel::default()
        } else {
            deduped[0]
        };
        Self {
            current,
            available: deduped,
        }
    }

    /// 当前生效档位。
    pub fn current(&self) -> PermissionLevel {
        self.current
    }

    /// 当前 Agent 支持的档位（按声明顺序）。
    pub fn available(&self) -> &[PermissionLevel] {
        &self.available
    }

    /// 当前档位的呈现元数据。
    pub fn meta(&self) -> &'static PermissionMeta {
        meta(self.current)
    }

    /// 当前档位的非 Emoji 图标字形。
    pub fn glyph(&self) -> &'static str {
        self.meta().glyph
    }

    /// 按钮无障碍文案：`执行权限：询问确认（Alt+P 循环）`。
    pub fn aria_label(&self) -> String {
        format!("执行权限：{}（Alt+P 循环）", self.meta().label)
    }

    /// 悬停说明：中文标签 + 完整语义 + 循环快捷键。
    pub fn hint(&self) -> String {
        format!(
            "{} — {}（Alt+P 循环切换）",
            self.meta().label,
            self.meta().description
        )
    }

    /// 宽版是否展示中文标签（超窄屏退化为纯图标钮）。
    pub fn show_label(&self, geometry: &super::metrics::ComposerGeometry) -> bool {
        geometry.show_permission_label
    }

    /// 循环切换到相邻支持档位并返回新档位。
    pub fn cycle(&mut self, direction: CycleDirection) -> PermissionLevel {
        let index = self
            .available
            .iter()
            .position(|l| *l == self.current)
            // 当前档位不在支持集合内（能力刚变化）时，从集合首项重新开始。
            .unwrap_or(0);
        let len = self.available.len();
        let next = match direction {
            CycleDirection::Forward => (index + 1) % len,
            CycleDirection::Backward => (index + len - 1) % len,
        };
        self.current = self.available[next];
        self.current
    }

    /// 尝试直接切到某档：不支持时回退到 `Ask`（或集合首项）并报告回退。
    pub fn try_set(&mut self, level: PermissionLevel) -> PermissionSelection {
        if level == self.current {
            return PermissionSelection::Unchanged(level);
        }
        if self.available.contains(&level) {
            self.current = level;
            return PermissionSelection::Applied(level);
        }
        let fallback = if self.available.contains(&PermissionLevel::Ask) {
            PermissionLevel::Ask
        } else {
            self.available[0]
        };
        self.current = fallback;
        PermissionSelection::Fallback {
            requested: level,
            applied: fallback,
        }
    }
}

impl Default for PermissionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::metrics::ComposerGeometry;
    use crate::layout::LayoutPlan;

    /// 6 档覆盖完整且顺序稳定：每个枚举变体恰好出现一次。
    #[test]
    fn catalog_covers_all_six_levels_once() {
        assert_eq!(PERMISSION_CATALOG.len(), 6);
        for level in PERMISSION_ORDER {
            let hits = PERMISSION_CATALOG
                .iter()
                .filter(|m| m.level == level)
                .count();
            assert_eq!(hits, 1, "{level:?} 在目录中应恰好出现一次");
        }
        // 中文标签与说明均非空，且不是 Emoji。
        for m in PERMISSION_CATALOG {
            assert!(!m.label.is_empty());
            assert!(!m.description.is_empty());
            assert_eq!(m.glyph.chars().count(), 1, "占位图标应为单字符");
        }
    }

    /// 默认档位为 `Ask`（与 `soxu-protocol` 的 `PermissionLevel::default()` 对齐）。
    #[test]
    fn default_level_is_ask() {
        let state = PermissionState::new();
        assert_eq!(state.current(), PermissionLevel::Ask);
        assert_eq!(state.available().len(), 6);
        assert_eq!(state.meta().label, "询问确认");
    }

    /// 正向循环 6 步回到原点，反向 1 步可逆。
    #[test]
    fn forward_cycle_wraps_after_six_steps() {
        let mut state = PermissionState::new();
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(state.cycle(CycleDirection::Forward));
        }
        // Ask 起点 → AutoEdit … Turbo → Plan … Ask，6 步回到起点。
        assert_eq!(state.current(), PermissionLevel::Ask);
        assert_eq!(seen.len(), 6);
        assert_eq!(
            seen,
            vec![
                PermissionLevel::AutoEdit,
                PermissionLevel::FullAccess,
                PermissionLevel::Turbo,
                PermissionLevel::Plan,
                PermissionLevel::ReadOnly,
                PermissionLevel::Ask,
            ]
        );

        // 反向一步从 Ask 回到 ReadOnly（保守档），与正向 5 步等价。
        state.cycle(CycleDirection::Backward);
        assert_eq!(state.current(), PermissionLevel::ReadOnly);
    }

    /// 能力子集循环：不支持的档位被自然跳过。
    #[test]
    fn cycle_skips_unsupported_levels() {
        let mut state = PermissionState::for_agent([
            PermissionLevel::Ask,
            PermissionLevel::AutoEdit,
            PermissionLevel::FullAccess,
        ]);
        assert_eq!(state.available().len(), 3);
        assert_eq!(
            state.cycle(CycleDirection::Forward),
            PermissionLevel::AutoEdit
        );
        assert_eq!(
            state.cycle(CycleDirection::Forward),
            PermissionLevel::FullAccess
        );
        // 回绕时不会落到不支持的 Plan / ReadOnly / Turbo。
        assert_eq!(state.cycle(CycleDirection::Forward), PermissionLevel::Ask);
    }

    /// 空能力集合回退到 `Ask`，且去重稳定。
    #[test]
    fn empty_capability_falls_back_to_ask() {
        let state = PermissionState::for_agent([]);
        assert_eq!(state.current(), PermissionLevel::Ask);
        assert_eq!(state.available(), &[PermissionLevel::Ask]);

        let deduped = PermissionState::for_agent([
            PermissionLevel::AutoEdit,
            PermissionLevel::AutoEdit,
            PermissionLevel::Ask,
        ]);
        assert_eq!(
            deduped.available(),
            &[PermissionLevel::AutoEdit, PermissionLevel::Ask]
        );
        // `Ask` 在集合内 → 保持默认档位。
        assert_eq!(deduped.current(), PermissionLevel::Ask);
    }

    /// 不支持的目标档位回退到 `Ask` 并给出回退报告（对齐 `lost-controls`）。
    #[test]
    fn unsupported_set_falls_back_to_ask() {
        let mut state =
            PermissionState::for_agent([PermissionLevel::Ask, PermissionLevel::AutoEdit]);
        assert_eq!(
            state.try_set(PermissionLevel::Turbo),
            PermissionSelection::Fallback {
                requested: PermissionLevel::Turbo,
                applied: PermissionLevel::Ask,
            }
        );
        assert_eq!(state.current(), PermissionLevel::Ask);

        assert_eq!(
            state.try_set(PermissionLevel::AutoEdit),
            PermissionSelection::Applied(PermissionLevel::AutoEdit)
        );
        // 重复设置相同档位为幂等。
        assert_eq!(
            state.try_set(PermissionLevel::AutoEdit),
            PermissionSelection::Unchanged(PermissionLevel::AutoEdit)
        );
    }

    /// 能力集合不含 `Ask` 时，回退到集合首项（最保守可用档位）。
    #[test]
    fn fallback_prefers_first_available_without_ask() {
        let mut state =
            PermissionState::for_agent([PermissionLevel::FullAccess, PermissionLevel::Turbo]);
        assert_eq!(state.current(), PermissionLevel::FullAccess);
        assert_eq!(
            state.try_set(PermissionLevel::Plan),
            PermissionSelection::Fallback {
                requested: PermissionLevel::Plan,
                applied: PermissionLevel::FullAccess,
            }
        );
    }

    /// 无障碍文案：中文标签 + 快捷键，且不含中圆点分隔符。
    #[test]
    fn aria_and_hint_are_chinese_and_dot_free() {
        let state = PermissionState::new();
        let aria = state.aria_label();
        assert!(aria.contains("询问确认"));
        assert!(aria.contains("Alt+P"));
        assert!(!aria.contains('·'));

        let hint = state.hint();
        assert!(hint.contains("文件写入或命令执行前请求人工确认"));
        assert!(!hint.contains('·'));
    }

    /// 纯图标退化：超窄几何不展示中文标签。
    #[test]
    fn label_visibility_follows_geometry() {
        let state = PermissionState::new();
        let ultra = LayoutPlan::for_width(320.0);
        let narrow = ComposerGeometry::for_default_panels(320.0, &ultra);
        assert!(!state.show_label(&narrow));

        let wide = LayoutPlan::for_width(1280.0);
        let roomy = ComposerGeometry::for_default_panels(1280.0, &wide);
        assert!(state.show_label(&roomy));
    }
}
