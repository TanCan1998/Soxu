//! 单个 PTY 会话：输出泵、分片序号、尺寸调整与幂等关闭。
//!
//! 输出泵跑在独立系统线程上（PTY 主端读是阻塞 IO），事件出口同步下发；
//! 序号只在泵线程与关闭路径之间通过原子量协调，保证从 0 起严格递增、无跳号。

use crate::pty::{PTY_CLOSE_GRACE, PtyChild, PtyControl, PtyEventSink};
use chrono::Utc;
use soxu_protocol::{
    ActionableError, ErrorCode, PtyCloseReason, PtyEvent, PtyExitStatus, PtyInputData,
    PtyOutputChunk, PtyOutputData, PtySessionId, PtySessionSnapshot, PtySessionState, PtySize,
};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
/// 退出状态轮询间隔。
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 一个受控 PTY 会话。
pub struct PtySession {
    id: PtySessionId,
    snapshot: Mutex<PtySessionSnapshot>,
    control: Arc<dyn PtyControl>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    sink: Arc<dyn PtyEventSink>,
    sequence: AtomicU64,
    closed: AtomicBool,
    exit_emitted: AtomicBool,
    stream_lock: Mutex<()>,
    last_activity: Mutex<Instant>,
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtySession")
            .field("id", &self.id.as_str())
            .field("sequence", &self.sequence.load(Ordering::Relaxed))
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl PtySession {
    /// 启动会话并拉起输出泵。
    pub fn start(
        id: PtySessionId,
        workspace_id: String,
        size: PtySize,
        child: PtyChild,
        sink: Arc<dyn PtyEventSink>,
    ) -> Arc<Self> {
        let PtyChild {
            reader,
            writer,
            control,
        } = child;
        let now = Utc::now();
        let snapshot = PtySessionSnapshot {
            pty_id: id.clone(),
            workspace_id,
            size,
            state: PtySessionState::Running,
            exit: None,
            created_at: now,
            updated_at: now,
        };
        let session = Arc::new(Self {
            id,
            snapshot: Mutex::new(snapshot),
            control,
            writer: Mutex::new(Some(writer)),
            sink,
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            exit_emitted: AtomicBool::new(false),
            stream_lock: Mutex::new(()),
            last_activity: Mutex::new(Instant::now()),
        });
        let pump = Arc::clone(&session);
        if let Err(err) = std::thread::Builder::new()
            .name(format!("soxu-pty-{}", session.id.as_str()))
            .spawn(move || run_pump(pump, reader))
        {
            tracing::error!(kind = ?err.kind(), "终端输出泵线程创建失败");
            session.fail();
        }
        session
    }

    /// 会话标识。
    #[must_use]
    pub fn id(&self) -> &PtySessionId {
        &self.id
    }

    /// 当前快照。
    #[must_use]
    pub fn snapshot(&self) -> PtySessionSnapshot {
        lock(&self.snapshot).clone()
    }

    /// 是否已关闭。
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// 是否已超过空闲阈值（仅运行中会话可回收）。
    #[must_use]
    pub fn is_idle(&self, now: Instant, timeout: Duration) -> bool {
        if self.is_closed() {
            return false;
        }
        if lock(&self.snapshot).state != PtySessionState::Running {
            return false;
        }
        now.duration_since(*lock(&self.last_activity)) >= timeout
    }

    /// 写入输入；会话已关闭时拒绝。
    pub fn write(&self, data: &PtyInputData) -> Result<u32, ActionableError> {
        if self.is_closed() {
            return Err(closed_session());
        }
        {
            let mut guard = lock(&self.writer);
            let Some(writer) = guard.as_mut() else {
                return Err(closed_session());
            };
            writer
                .write_all(data.as_str().as_bytes())
                .and_then(|()| writer.flush())
                .map_err(|err| {
                    ActionableError::new(
                        ErrorCode::Conflict,
                        "终端输入写入失败",
                        "本次输入未送达；会话仍然存在",
                        "请重试或重新打开终端会话",
                    )
                    .with_diagnostic(format!("write={:?}", err.kind()))
                })?;
        }
        self.touch();
        Ok(u32::try_from(data.as_str().len()).unwrap_or(0))
    }

    /// 调整栅格尺寸并下发 `Resized` 事件。
    pub fn resize(&self, size: PtySize) -> Result<PtySessionSnapshot, ActionableError> {
        if self.is_closed() {
            return Err(closed_session());
        }
        self.control.resize(size)?;
        let snapshot = {
            let mut guard = lock(&self.snapshot);
            guard.size = size;
            guard.updated_at = Utc::now();
            guard.clone()
        };
        self.touch();
        self.sink.emit(PtyEvent::Resized(snapshot.clone()));
        Ok(snapshot)
    }

    /// 关闭会话：重复关闭返回当前快照，不报错、不重复下发事件。
    pub fn close(&self, reason: PtyCloseReason) -> Result<PtySessionSnapshot, ActionableError> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(self.snapshot());
        }
        let terminate_error = self.control.terminate().err();
        self.finish_stream();
        let snapshot = {
            let mut guard = lock(&self.snapshot);
            guard.state = if terminate_error.is_some() {
                PtySessionState::Failed
            } else {
                PtySessionState::Closed
            };
            guard.updated_at = Utc::now();
            guard.clone()
        };
        if let Some(error) = terminate_error {
            tracing::warn!(
                pty_id = self.id.as_str(),
                reason = ?reason,
                error = %error,
                "终端会话终止未确认"
            );
        }
        self.sink.emit(PtyEvent::Closed(snapshot.clone()));
        Ok(snapshot)
    }

    fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    fn emit_output(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let first = self.sequence.load(Ordering::Acquire);
        let mut chunks = PtyOutputChunk::split_output(&self.id, first, text);
        let count = u64::try_from(chunks.len()).unwrap_or(0);
        for chunk in &mut chunks {
            chunk.end_of_stream = false;
        }
        self.sequence.store(first + count, Ordering::Release);
        for chunk in chunks {
            self.sink.emit(PtyEvent::Output(chunk));
        }
    }

    /// 结束输出流：补一个 `end_of_stream` 终结片并下发 `Exited`（只发一次）。
    fn finish_stream(&self) {
        let _guard = lock(&self.stream_lock);
        if self.exit_emitted.swap(true, Ordering::AcqRel) {
            return;
        }
        let last = self.sequence.load(Ordering::Acquire);
        if let Ok(data) = PtyOutputData::try_from(String::new()) {
            self.sink.emit(PtyEvent::Output(PtyOutputChunk {
                pty_id: self.id.clone(),
                sequence: last,
                data,
                end_of_stream: true,
            }));
            self.sequence.store(last + 1, Ordering::Release);
        }
        let exit = self.collect_exit();
        {
            let mut guard = lock(&self.snapshot);
            if !matches!(
                guard.state,
                PtySessionState::Closed | PtySessionState::Failed
            ) {
                guard.state = PtySessionState::Exited;
            }
            guard.exit = exit;
            guard.updated_at = Utc::now();
        }
        self.sink.emit(PtyEvent::Exited {
            pty_id: self.id.clone(),
            exit: exit.unwrap_or(PtyExitStatus {
                exit_code: None,
                exit_signal: None,
            }),
            last_sequence: last,
        });
    }

    fn collect_exit(&self) -> Option<PtyExitStatus> {
        let deadline = Instant::now() + PTY_CLOSE_GRACE;
        loop {
            match self.control.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => {}
                Err(_) => return None,
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(EXIT_POLL_INTERVAL);
        }
    }

    fn fail(&self) {
        self.closed.store(true, Ordering::Release);
        let mut guard = lock(&self.snapshot);
        guard.state = PtySessionState::Failed;
        guard.updated_at = Utc::now();
    }
}

/// 输出泵：阻塞读取主端字节，按 UTF-8 字符边界解码后分片下发。
fn run_pump(session: Arc<PtySession>, mut reader: Box<dyn Read + Send>) {
    let mut buffer = [0u8; 8192];
    let mut decoder = Utf8Stream::default();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                session.touch();
                let text = decoder.push(&buffer[..read]);
                session.emit_output(&text);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let tail = decoder.finish();
    session.emit_output(&tail);
    session.finish_stream();
}

/// 增量 UTF-8 解码：只吐出完整字符，尾部不完整字节留到下一批。
///
/// PTY 输出按字节流到达，多字节字符可能被读窗口截断；直接按 `from_utf8_lossy`
/// 逐批解码会在每次截断处产生替换字符，因此必须保留尾部残字节。
#[derive(Debug, Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// 追加一批字节并返回已可安全切片的完整文本。
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        self.drain_complete()
    }

    /// 收尾：把残留的不完整字节按替换字符吐出，避免静默丢字节。
    pub fn finish(&mut self) -> String {
        let mut output = self.drain_complete();
        if !self.pending.is_empty() {
            output.push('\u{FFFD}');
            self.pending.clear();
        }
        output
    }

    fn drain_complete(&mut self) -> String {
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    output.push_str(text);
                    self.pending.clear();
                    return output;
                }
                Err(err) if err.error_len().is_none() => {
                    let valid = err.valid_up_to();
                    if let Ok(text) = std::str::from_utf8(&self.pending[..valid]) {
                        output.push_str(text);
                    }
                    self.pending.drain(..valid);
                    return output;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    if let Ok(text) = std::str::from_utf8(&self.pending[..valid]) {
                        output.push_str(text);
                    }
                    output.push('\u{FFFD}');
                    let skip = valid + err.error_len().unwrap_or(1);
                    let skip = skip.min(self.pending.len());
                    self.pending.drain(..skip);
                }
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn closed_session() -> ActionableError {
    ActionableError::new(
        ErrorCode::Conflict,
        "终端会话已关闭",
        "本次输入未写入任何进程",
        "请重新打开终端会话后重试",
    )
}
