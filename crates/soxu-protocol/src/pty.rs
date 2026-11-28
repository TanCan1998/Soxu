//! 受控 PTY 契约：会话打开、输入、尺寸调整、关闭的请求与响应，
//! 以及有界分片输出与退出事件（M6 前置契约）。
//!
//! 校验红线：输出只允许有界分片并携带单调序号，禁止无界单事件；
//! 输入同样有界；尺寸、工作目录与环境白名单策略在反序列化阶段校验。
//!
//! 本模块只定义契约：真实 PTY 派生、帧编解码与背压由后续 daemon 实现 PR 落地。

use crate::primitives::{EnvKey, WorkspaceRelativePath};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 单个输出分片最大字节数（16 KiB）。
pub const PTY_OUTPUT_CHUNK_MAX_BYTES: usize = 16 * 1024;
/// 单次输入最大字节数（8 KiB）。
pub const PTY_INPUT_CHUNK_MAX_BYTES: usize = 8 * 1024;
/// 环境白名单最大条数。
pub const PTY_ENV_ALLOWLIST_MAX_COUNT: usize = 64;
/// 终端最大列数。
pub const PTY_MAX_COLUMNS: u16 = 1_000;
/// 终端最大行数。
pub const PTY_MAX_ROWS: u16 = 500;

/// PTY 会话标识：服务端签发，字符集限制在 URL 与日志安全范围内。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PtySessionId(String);

impl PtySessionId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PtySessionId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > 128 {
            return Err("PTY 会话标识长度必须在 1..=128 字节之间");
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        {
            return Err("PTY 会话标识只允许 ASCII 字母、数字、`-`、`_`、`.` 与 `:`");
        }
        Ok(Self(value))
    }
}

impl From<PtySessionId> for String {
    fn from(value: PtySessionId) -> Self {
        value.0
    }
}

/// PTY 栅格尺寸：列 1..=1000，行 1..=500。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PtySizeWire")]
pub struct PtySize {
    pub columns: u16,
    pub rows: u16,
}

#[derive(Deserialize)]
struct PtySizeWire {
    columns: u16,
    rows: u16,
}

impl TryFrom<PtySizeWire> for PtySize {
    type Error = &'static str;

    fn try_from(wire: PtySizeWire) -> Result<Self, Self::Error> {
        if !(1..=PTY_MAX_COLUMNS).contains(&wire.columns) {
            return Err("PTY 列数必须落在 1..=1000");
        }
        if !(1..=PTY_MAX_ROWS).contains(&wire.rows) {
            return Err("PTY 行数必须落在 1..=500");
        }
        Ok(Self {
            columns: wire.columns,
            rows: wire.rows,
        })
    }
}

impl PtySize {
    /// 构造经过边界校验的栅格尺寸。
    pub fn new(columns: u16, rows: u16) -> Result<Self, &'static str> {
        Self::try_from(PtySizeWire { columns, rows })
    }
}

/// 单个输出分片负载：UTF-8 文本，字节数不超过 [`PTY_OUTPUT_CHUNK_MAX_BYTES`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PtyOutputData(String);

impl PtyOutputData {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.0.len()
    }
}

impl TryFrom<String> for PtyOutputData {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > PTY_OUTPUT_CHUNK_MAX_BYTES {
            return Err("PTY 输出分片不得超过 16384 字节");
        }
        Ok(Self(value))
    }
}

impl From<PtyOutputData> for String {
    fn from(value: PtyOutputData) -> Self {
        value.0
    }
}

/// 单次输入负载：非空且字节数不超过 [`PTY_INPUT_CHUNK_MAX_BYTES`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PtyInputData(String);

impl PtyInputData {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.0.len()
    }
}

impl TryFrom<String> for PtyInputData {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("PTY 输入不得为空");
        }
        if value.len() > PTY_INPUT_CHUNK_MAX_BYTES {
            return Err("单次 PTY 输入不得超过 8192 字节，请拆分为多次写入");
        }
        Ok(Self(value))
    }
}

impl From<PtyInputData> for String {
    fn from(value: PtyInputData) -> Self {
        value.0
    }
}

/// PTY 起点目录策略：工作区根或工作区内子目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PtyWorkdirPolicy {
    WorkspaceRoot,
    Subdirectory {
        relative_path: WorkspaceRelativePath,
    },
}

/// PTY 环境白名单策略：只按名放行，值由服务端从受控来源注入，绝不回传原始环境。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PtyEnvPolicyWire")]
pub struct PtyEnvPolicy {
    pub allow_keys: Vec<EnvKey>,
    pub inherits_parent_env: bool,
}

#[derive(Deserialize)]
struct PtyEnvPolicyWire {
    #[serde(default)]
    allow_keys: Vec<EnvKey>,
    #[serde(default)]
    inherits_parent_env: bool,
}

impl TryFrom<PtyEnvPolicyWire> for PtyEnvPolicy {
    type Error = &'static str;

    fn try_from(wire: PtyEnvPolicyWire) -> Result<Self, Self::Error> {
        if wire.allow_keys.len() > PTY_ENV_ALLOWLIST_MAX_COUNT {
            return Err("PTY 环境白名单最多 64 条");
        }
        let mut seen = HashSet::with_capacity(wire.allow_keys.len());
        if wire.allow_keys.iter().any(|key| !seen.insert(key.as_str())) {
            return Err("PTY 环境白名单禁止重复项");
        }
        Ok(Self {
            allow_keys: wire.allow_keys,
            inherits_parent_env: wire.inherits_parent_env,
        })
    }
}

/// PTY 会话状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PtySessionState {
    Starting,
    Running,
    Exited,
    Closed,
    Failed,
}

/// 进程退出信息：正常退出给退出码，被信号终止给信号号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtyExitStatus {
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
}

/// PTY 会话快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtySessionSnapshot {
    pub pty_id: PtySessionId,
    pub workspace_id: String,
    pub size: PtySize,
    pub state: PtySessionState,
    pub exit: Option<PtyExitStatus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 打开 PTY 会话请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPtyRequest {
    pub workspace_id: String,
    pub size: PtySize,
    pub workdir: PtyWorkdirPolicy,
    #[serde(default)]
    pub env_policy: PtyEnvPolicy,
}

/// 打开 PTY 会话响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPtyResponse {
    pub session: PtySessionSnapshot,
}

/// 写入 PTY 输入请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritePtyRequest {
    pub pty_id: PtySessionId,
    pub data: PtyInputData,
}

/// 写入 PTY 输入响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritePtyResponse {
    pub pty_id: PtySessionId,
    pub accepted_bytes: u32,
}

/// 调整 PTY 尺寸请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResizePtyRequest {
    pub pty_id: PtySessionId,
    pub size: PtySize,
}

/// 调整 PTY 尺寸响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResizePtyResponse {
    pub session: PtySessionSnapshot,
}

/// 关闭原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PtyCloseReason {
    ClientRequest,
    IdleTimeout,
    ServerShutdown,
    Error,
}

/// 关闭 PTY 会话请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePtyRequest {
    pub pty_id: PtySessionId,
    pub reason: PtyCloseReason,
}

/// 关闭 PTY 会话响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePtyResponse {
    pub session: PtySessionSnapshot,
}

/// 有界输出分片：`sequence` 从 0 起严格递增，客户端按序拼接并在缺口时请求快照重放。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtyOutputChunk {
    pub pty_id: PtySessionId,
    pub sequence: u64,
    pub data: PtyOutputData,
    /// 流结束标记：收到后不得再期待更大 `sequence`。
    pub end_of_stream: bool,
}

impl PtyOutputChunk {
    /// 把一段输出切成有界分片，`first_sequence` 为起始序号。
    ///
    /// 切分严格落在 UTF-8 字符边界，绝不切断多字节字符。
    #[must_use]
    pub fn split_output(pty_id: &PtySessionId, first_sequence: u64, output: &str) -> Vec<Self> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for ch in output.chars() {
            if !current.is_empty() && current.len() + ch.len_utf8() > PTY_OUTPUT_CHUNK_MAX_BYTES {
                let sequence = first_sequence + chunks.len() as u64;
                chunks.push(Self {
                    pty_id: pty_id.clone(),
                    sequence,
                    data: PtyOutputData(std::mem::take(&mut current)),
                    end_of_stream: false,
                });
            }
            current.push(ch);
        }
        let sequence = first_sequence + chunks.len() as u64;
        chunks.push(Self {
            pty_id: pty_id.clone(),
            sequence,
            data: PtyOutputData(current),
            end_of_stream: true,
        });
        chunks
    }
}

/// PTY 域事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PtyEvent {
    Output(PtyOutputChunk),
    Resized(PtySessionSnapshot),
    Exited {
        pty_id: PtySessionId,
        exit: PtyExitStatus,
        /// 退出前的最后一个输出分片序号，便于客户端校验是否漏帧。
        last_sequence: u64,
    },
    Closed(PtySessionSnapshot),
}

/// PTY 域命令（由全局 `RpcCommand` 在实现 PR 中 1:1 转发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum PtyCommand {
    Open(OpenPtyRequest),
    Write(WritePtyRequest),
    Resize(ResizePtyRequest),
    Close(ClosePtyRequest),
}

/// PTY 域命令结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PtyCommandResult {
    Opened(OpenPtyResponse),
    Written(WritePtyResponse),
    Resized(ResizePtyResponse),
    Closed(ClosePtyResponse),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_id() -> PtySessionId {
        PtySessionId::try_from("pty_01".to_owned()).expect("会话标识应合法")
    }

    #[test]
    fn split_output_respects_byte_budget_and_char_boundaries() {
        let chunk_text = "终端输出🙂".repeat(5_000);
        let chunks = PtyOutputChunk::split_output(&session_id(), 7, &chunk_text);
        assert!(chunks.len() > 1, "超限输出必须切成多片");
        for (index, chunk) in chunks.iter().enumerate() {
            assert!(
                chunk.data.byte_len() <= PTY_OUTPUT_CHUNK_MAX_BYTES,
                "分片 {index} 超出预算"
            );
            assert_eq!(chunk.sequence, 7 + index as u64, "序号必须连续递增");
            assert_eq!(
                chunk.end_of_stream,
                index + 1 == chunks.len(),
                "只有最后一片标记流结束"
            );
        }
        let rejoined: String = chunks
            .iter()
            .map(|chunk| chunk.data.as_str().to_owned())
            .collect();
        assert_eq!(rejoined, chunk_text, "分片必须可无损重组");
    }

    #[test]
    fn split_output_keeps_empty_output_as_single_end_chunk() {
        let chunks = PtyOutputChunk::split_output(&session_id(), 0, "");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].end_of_stream);
        assert!(chunks[0].data.as_str().is_empty());
    }

    #[test]
    fn pty_size_rejects_zero_and_out_of_range() {
        assert!(PtySize::new(0, 24).is_err());
        assert!(PtySize::new(80, 0).is_err());
        assert!(PtySize::new(PTY_MAX_COLUMNS + 1, 24).is_err());
        assert!(PtySize::new(80, PTY_MAX_ROWS + 1).is_err());
        assert_eq!(
            PtySize::new(120, 40).expect("合法尺寸"),
            PtySize {
                columns: 120,
                rows: 40
            }
        );
    }
}
