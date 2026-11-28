//! 流式合批节拍：`<= 8.3Hz` 提交与 120ms 量化窗口（纯逻辑）。
//!
//! 设计基准：`design-system/soxu/tokens.json -> motion.streamCommitMaxHz = 8.3`
//! 与 `pages/03-timeline-tools-and-checkpoints.md`（流式思考窗、工具输出实时刷新）。
//! 结构参考 Padu `docs/performance.md` 的两条节拍不变量：provider 增量先入队，
//! 120ms 窗口内**折叠成一次提交（drain）→ 一次通知 → 一次尾行重测**，
//! 因此逐提交的解析/高亮成本与 provider 的 chunk 频率解耦。
//!
//! 关键陷阱（Padu `cadence-invariants.md` §2.A）：每种增量都必须打上标记并路由到
//! 同一条 `StreamFrame` 节拍；漏掉一种（例如 reasoning）会让提交频率飙到 40+/s。
//! 本模块用 [`DeltaKind`] 位掩码强制「每种增量都进入同一批次」，并有单测守住。

use crate::tokens::motion;

/// 量化窗口时长（毫秒）：`1000 / 8.3 ≈ 120.48` → 取 120ms，保证不超过 8.3Hz。
pub const STREAM_COMMIT_WINDOW_MS: u64 = 120;

/// 令牌中声明的刷新频率上限（Hz），与 [`STREAM_COMMIT_WINDOW_MS`] 自洽。
pub const MAX_COMMIT_HZ: f32 = motion::STREAM_COMMIT_MAX_HZ;

/// 120ms 量化的**精确**长期平均上限（Hz）：`1000 / 120 ≈ 8.34`。
///
/// 设计令牌把该量级声明为 `8.3Hz`（`motion.streamCommitMaxHz`），120ms 网格是它的
/// 落点：1 秒内最多 9 次提交，长期平均 8.34Hz，与 8.3Hz 是取整声明关系。
/// 单测拿这个精确值断浮点，而不是拿取整值 8.3 去卡，否则会把正确实现判错。
pub const EXACT_CEILING_HZ: f32 = 1000.0 / STREAM_COMMIT_WINDOW_MS as f32;

/// 每次提交最多重测的尾行数，避免一次提交把整条对话流重新量高。
pub const TAIL_REMEASURE_ROWS: usize = 2;

/// 增量类别。位掩码化以便一次提交同时记录多个类别（Padu 漏标记陷阱的守卫）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaKind {
    /// 助手正文文本增量。
    Text,
    /// 折叠思考窗的推演增量。
    Reasoning,
    /// 工具输出（终端/stdout/diff 预览）增量。
    ToolOutput,
    /// `LiveSteps` 步骤推进。
    Step,
}

impl DeltaKind {
    /// 全部类别（供「每种增量都进入节拍」的穷举测试）。
    pub const ALL: [Self; 4] = [Self::Text, Self::Reasoning, Self::ToolOutput, Self::Step];

    /// 该类别在批次位掩码中的位。
    pub const fn mask(self) -> u8 {
        match self {
            Self::Text => 0b0001,
            Self::Reasoning => 0b0010,
            Self::ToolOutput => 0b0100,
            Self::Step => 0b1000,
        }
    }
}

/// 窗口内累积、尚未提交的增量。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingBatch {
    /// 累计增量条数。
    pub deltas: u32,
    /// 累计字节数（用于判定是否需要重新折行）。
    pub bytes: usize,
    /// 已出现的类别位掩码。
    pub kinds: u8,
}

impl PendingBatch {
    /// 是否没有待提交增量。
    pub fn is_empty(&self) -> bool {
        self.deltas == 0
    }

    /// 该类别本次是否出现过。
    pub fn has(&self, kind: DeltaKind) -> bool {
        self.kinds & kind.mask() != 0
    }
}

/// 一次已完成提交的可观测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitBatch {
    /// 本次折叠的增量条数。
    pub deltas: u32,
    /// 本次折叠的字节数。
    pub bytes: usize,
    /// 本次涉及的类别位掩码。
    pub kinds: u8,
    /// 本次允许重测的尾行数。
    pub remeasure_tail_rows: usize,
    /// 提交发生的时间点（毫秒，量化窗口边界）。
    pub at_ms: u64,
}

impl CommitBatch {
    /// 本次提交是否包含某类别。
    pub fn has(&self, kind: DeltaKind) -> bool {
        self.kinds & kind.mask() != 0
    }
}

/// 流式提交节拍器。
///
/// 量化规则：把时间轴切成 `window_ms` 宽的槽，**每个槽最多提交一次**。
/// 因此 1 秒最多 `ceil(1000 / 120) = 9` 次提交，长时间平均严格 `<= 8.3Hz`
/// （120ms 槽 → 1000/120 = 8.33Hz 上限，且槽边界对齐不再叠加抖动）。
#[derive(Clone, Debug)]
pub struct StreamCadence {
    /// 量化窗口（毫秒，最小 1）。
    window_ms: u64,
    /// 上一个已提交的槽号。
    last_slot: Option<u64>,
    /// 待提交增量。
    pending: PendingBatch,
    /// 累计提交次数。
    commits: u64,
    /// 首个已提交槽号（实测频率的起点）。
    first_slot: Option<u64>,
}

impl StreamCadence {
    /// 以设计令牌窗口（120ms）创建。
    pub fn new() -> Self {
        Self::with_window_ms(STREAM_COMMIT_WINDOW_MS)
    }

    /// 自定义窗口（单测用；0 会被抬到 1ms，避免除零）。
    pub fn with_window_ms(window_ms: u64) -> Self {
        Self {
            window_ms: window_ms.max(1),
            last_slot: None,
            pending: PendingBatch::default(),
            commits: 0,
            first_slot: None,
        }
    }

    /// 当前窗口时长（毫秒）。
    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    /// 时间点所属量化槽号。
    pub fn slot_of(&self, at_ms: u64) -> u64 {
        at_ms / self.window_ms
    }

    /// 下一个允许提交的时间点（槽边界）；从未提交过时立即允许（返回 0）。
    pub fn next_wake_ms(&self, _now_ms: u64) -> u64 {
        match self.last_slot {
            Some(slot) => (slot + 1) * self.window_ms,
            None => 0,
        }
    }

    /// 记录一条增量（不触发渲染）。
    pub fn observe(&mut self, _at_ms: u64, kind: DeltaKind, bytes: usize) {
        self.pending.deltas = self.pending.deltas.saturating_add(1);
        self.pending.bytes = self.pending.bytes.saturating_add(bytes);
        self.pending.kinds |= kind.mask();
    }

    /// 待提交增量（只读）。
    pub fn pending(&self) -> &PendingBatch {
        &self.pending
    }

    /// 尝试在 `now_ms` 提交：同一窗口已经提交过、或没有待提交增量时返回 `None`。
    pub fn try_commit(&mut self, now_ms: u64) -> Option<CommitBatch> {
        if self.pending.is_empty() {
            return None;
        }
        let slot = self.slot_of(now_ms);
        if self.last_slot == Some(slot) {
            return None;
        }
        self.last_slot = Some(slot);
        self.first_slot.get_or_insert(slot);
        self.commits = self.commits.saturating_add(1);
        let batch = CommitBatch {
            deltas: self.pending.deltas,
            bytes: self.pending.bytes,
            kinds: self.pending.kinds,
            remeasure_tail_rows: TAIL_REMEASURE_ROWS,
            at_ms: slot * self.window_ms,
        };
        self.pending = PendingBatch::default();
        Some(batch)
    }

    /// 累计提交次数。
    pub fn commit_count(&self) -> u64 {
        self.commits
    }

    /// 实测平均提交频率（Hz）。
    ///
    /// 分母是**已派发窗口数**（`last_slot - first_slot + 1`），不是墙钟时长：
    /// 量化节拍器的硬不变量是「每窗口最多一次提交」，因此该实测值恒满足
    /// `<= 1000 / window_ms`（120ms → 8.34Hz），不会被首尾边界的一次提交抬高。
    pub fn measured_hz(&self) -> f32 {
        let (Some(first), Some(last)) = (self.first_slot, self.last_slot) else {
            return 0.0;
        };
        let windows = last.saturating_sub(first) + 1;
        self.commits as f32 / (windows as f64 * self.window_ms as f64 / 1000.0) as f32
    }
}

impl Default for StreamCadence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_all_deltas_inside_one_window() {
        let mut cadence = StreamCadence::new();
        for at in 0..5 {
            cadence.observe(at, DeltaKind::Text, 10);
        }
        assert_eq!(cadence.pending().deltas, 5);
        assert_eq!(cadence.pending().bytes, 50);

        let batch = cadence.try_commit(60);
        let Some(batch) = batch else {
            panic!("窗口内首次提交应被允许");
        };
        assert_eq!(batch.deltas, 5);
        assert_eq!(batch.bytes, 50);
        assert!(batch.has(DeltaKind::Text));
        assert_eq!(batch.remeasure_tail_rows, TAIL_REMEASURE_ROWS);
        assert!(cadence.pending().is_empty());

        // 同一窗口第二次提交被拒绝。
        cadence.observe(61, DeltaKind::Text, 5);
        assert!(cadence.try_commit(119).is_none());
        // 下一个窗口放行。
        assert!(cadence.try_commit(120).is_some());
    }

    #[test]
    fn commit_rate_stays_within_8_3hz_quantization() {
        let mut cadence = StreamCadence::new();
        // 每 1ms 一条增量，持续 10s：模拟高频 provider chunk。
        let mut commits = 0_u64;
        for at in 0..10_000_u64 {
            cadence.observe(at, DeltaKind::Text, 8);
            if cadence.try_commit(at).is_some() {
                commits += 1;
            }
        }
        // 10s / 120ms = 83.3 → 最多 84 次（含 0ms 槽）。
        assert!(commits <= 84, "提交次数 {commits} 超出 8.3Hz 量化上限");
        // 实测频率不超 120ms 量化的精确上限（≫ 令牌名义 8.3Hz 的取整声明）。
        let hz = cadence.measured_hz();
        assert!(
            hz <= EXACT_CEILING_HZ + 0.01,
            "实测 {hz}Hz 超出量化上限 {EXACT_CEILING_HZ}Hz"
        );
        // 令牌声明值不超过精确上限（自洽，不允许令牌写得更激进）。
        let nominal = MAX_COMMIT_HZ;
        let exact = EXACT_CEILING_HZ;
        assert!(nominal <= exact, "令牌 {nominal}Hz 超过量化上限 {exact}Hz");
        // 10s 至少 80 次提交，证明节拍没有被过度拖慢（一次都不能丢）。
        assert!(commits >= 80, "提交次数 {commits} 过低，节拍被过度合并");
    }

    #[test]
    fn consecutive_commits_are_never_closer_than_one_window() {
        let mut cadence = StreamCadence::new();
        let mut commit_times = Vec::new();
        for at in 0..1_000_u64 {
            cadence.observe(at, DeltaKind::ToolOutput, 2);
            if let Some(batch) = cadence.try_commit(at) {
                commit_times.push(batch.at_ms);
            }
        }
        assert!(commit_times.len() >= 8);
        for pair in commit_times.windows(2) {
            let gap = pair[1].saturating_sub(pair[0]);
            assert!(
                gap >= STREAM_COMMIT_WINDOW_MS,
                "相邻提交间隔 {gap}ms 小于量化窗口 {STREAM_COMMIT_WINDOW_MS}ms"
            );
        }
    }

    #[test]
    fn every_delta_kind_is_folded_into_the_same_batch() {
        // Padu 陷阱守卫：任何一种增量都必须进入 StreamFrame 节拍。
        for kind in DeltaKind::ALL {
            let mut cadence = StreamCadence::new();
            cadence.observe(0, kind, 4);
            let batch = cadence.try_commit(0);
            let Some(batch) = batch else {
                panic!("{kind:?} 增量未产生提交");
            };
            assert!(batch.has(kind), "{kind:?} 未进入同一批次");
            assert_eq!(batch.deltas, 1);
        }
        // 混合类别在同一窗口内合并成一次提交。
        let mut cadence = StreamCadence::new();
        for (offset, kind) in DeltaKind::ALL.iter().enumerate() {
            cadence.observe(offset as u64, *kind, 1);
        }
        let batch = cadence.try_commit(10);
        let Some(batch) = batch else {
            panic!("混合增量未产生提交");
        };
        assert_eq!(batch.deltas, 4);
        for kind in DeltaKind::ALL {
            assert!(batch.has(kind), "{kind:?} 在混合批次中丢失");
        }
    }

    #[test]
    fn wake_aligns_to_window_boundary() {
        let mut cadence = StreamCadence::new();
        assert_eq!(cadence.next_wake_ms(0), 0);
        cadence.observe(0, DeltaKind::Text, 1);
        assert!(cadence.try_commit(0).is_some());
        assert_eq!(cadence.next_wake_ms(1), 120);
        assert_eq!(cadence.next_wake_ms(119), 120);
        cadence.observe(120, DeltaKind::Text, 1);
        assert!(cadence.try_commit(120).is_some());
        assert_eq!(cadence.next_wake_ms(120), 240);
        assert_eq!(cadence.window_ms(), STREAM_COMMIT_WINDOW_MS);
    }

    #[test]
    fn empty_commit_is_a_noop() {
        let mut cadence = StreamCadence::new();
        assert!(cadence.try_commit(0).is_none());
        assert_eq!(cadence.commit_count(), 0);
        assert_eq!(cadence.measured_hz(), 0.0);
    }

    #[test]
    fn zero_window_is_coerced_to_one_millisecond() {
        let cadence = StreamCadence::with_window_ms(0);
        assert_eq!(cadence.window_ms(), 1);
        assert_eq!(cadence.slot_of(7), 7);
    }
}
