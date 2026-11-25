use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};

use crate::events::Decoder;
use crate::invocation::{Input, invocation};
use crate::{ControlSupport, DriverEvent, NativeCliKind, Outcome, RunRequest};

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("未找到原生 CLI")]
    BinaryNotFound,
    #[error("Windows 入口为脚本 shim，请指定原生 EXE 或 Node EXE 与 JS 入口")]
    ScriptShim,
    #[error("可执行文件与工作目录必须使用绝对路径")]
    RelativePath,
    #[error("无效运行参数：{0}")]
    InvalidRequest(&'static str),
    #[error("该适配器不支持：{0}")]
    Unsupported(&'static str),
    #[error("待处理控制请求已满")]
    ControlBackpressure,
    #[error("当前轮次已结束")]
    Closed,
    #[error("无法启动 CLI（{0:?}）")]
    Spawn(io::ErrorKind),
}

pub struct NativeDriver {
    kind: NativeCliKind,
    program: PathBuf,
    prefix: Vec<OsString>,
    environment: Vec<(OsString, Option<OsString>)>,
}

impl NativeDriver {
    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn discover(kind: NativeCliKind) -> Result<Self, DriverError> {
        let mut shim = false;
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path).filter(|dir| dir.is_absolute()) {
                let candidate = dir.join(kind.binary_name());
                if cfg!(windows) {
                    let exe = candidate.with_extension("exe");
                    if exe.is_file() {
                        return Self::with_program(kind, exe, []);
                    }
                    shim |= ["cmd", "bat", "ps1"]
                        .iter()
                        .any(|extension| candidate.with_extension(extension).is_file());
                } else if candidate.is_file() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt as _;
                        if !candidate
                            .metadata()
                            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
                        {
                            continue;
                        }
                    }
                    return Self::with_program(kind, candidate, []);
                }
            }
        }
        Err(if shim {
            DriverError::ScriptShim
        } else {
            DriverError::BinaryNotFound
        })
    }

    /// `prefix` 仅接受 Daemon 信任的静态启动参数，例如 Node 应运行的绝对 JS 路径。
    pub fn with_program(
        kind: NativeCliKind,
        program: PathBuf,
        prefix: impl IntoIterator<Item = OsString>,
    ) -> Result<Self, DriverError> {
        if !program.is_absolute() {
            return Err(DriverError::RelativePath);
        }
        if cfg!(windows)
            && !program
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(DriverError::ScriptShim);
        }
        Ok(Self {
            kind,
            program,
            prefix: prefix.into_iter().collect(),
            environment: Vec::new(),
        })
    }

    /// 仅对当前驱动的子进程设置或移除环境变量。
    pub fn with_env(mut self, key: impl Into<OsString>, value: Option<OsString>) -> Self {
        self.environment.push((key.into(), value));
        self
    }

    pub fn start(&self, request: RunRequest) -> Result<RunningTurn, DriverError> {
        let (args, input) = invocation(self.kind, &request)?;
        let directory = request.target.directory();
        if !directory.is_absolute() {
            return Err(DriverError::RelativePath);
        }
        let mut command = Command::new(&self.program);
        if self.kind == NativeCliKind::OpenCode {
            command.env("OPENCODE_PERMISSION", r#"{"*":"deny"}"#);
        }
        command
            .args(&self.prefix)
            .args(args)
            .current_dir(directory)
            .stdin(if matches!(input, Input::Argument) {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for (key, value) in &self.environment {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(DriverError::InvalidRequest("Tokio runtime"));
        }
        let mut child = command
            .spawn()
            .map_err(|err| DriverError::Spawn(err.kind()))?;
        let Some(stdout) = child.stdout.take() else {
            return Err(DriverError::Spawn(io::ErrorKind::BrokenPipe));
        };
        let stdin = child.stdin.take();
        let (events, receiver) = mpsc::channel(64);
        let (commands, pending) = mpsc::channel(16);
        let (cancel, mut cancelled) = watch::channel(false);
        let (active, activity) = watch::channel(false);
        let kind = self.kind;
        tokio::spawn(async move {
            let outcome = tokio::select! {
                biased;
                _ = cancelled.wait_for(|value| *value) => Outcome::Cancelled,
                _ = events.closed() => Outcome::Cancelled,
                result = async {
                    let result = drive(kind, input, request.prompt, stdin, stdout, pending, (&events, &active)).await;
                    match result {
                        Ok(outcome) => match child.wait().await {
                            Ok(status) if status.success() || outcome == Outcome::Cancelled => outcome,
                            Ok(status) => Outcome::ProcessFailed { code: status.code() },
                            Err(_) => Outcome::IoFailed,
                        },
                        Err(_) => Outcome::IoFailed,
                    }
                } => result,
            };
            active.send_replace(false);
            if matches!(outcome, Outcome::Cancelled | Outcome::IoFailed) {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            let _ = events.send(DriverEvent::Finished(outcome)).await;
        });
        Ok(RunningTurn {
            receiver,
            control: TurnControl {
                kind: self.kind,
                cancel,
                commands,
                activity,
            },
        })
    }
}

pub struct RunningTurn {
    receiver: mpsc::Receiver<DriverEvent>,
    control: TurnControl,
}

impl RunningTurn {
    pub async fn next_event(&mut self) -> Option<DriverEvent> {
        self.receiver.recv().await
    }

    pub fn control(&self) -> TurnControl {
        self.control.clone()
    }
}

#[derive(Clone)]
pub struct TurnControl {
    kind: NativeCliKind,
    cancel: watch::Sender<bool>,
    commands: mpsc::Sender<(u64, Control)>,
    activity: watch::Receiver<bool>,
}

pub enum Control {
    Queue(String),
    Steer(String),
    Interrupt,
}

impl TurnControl {
    pub fn is_active(&self) -> bool {
        *self.activity.borrow() && !*self.cancel.borrow() && !self.commands.is_closed()
    }

    /// 请求终止所属 CLI 进程。可跨任务调用，且不受输出通道背压影响。
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }

    /// 调用方为每个待处理控制请求提供唯一的 ID。Pi 的响应通过 `ControlAck` 返回。
    pub fn send(&self, id: u64, command: Control) -> Result<ControlSupport, DriverError> {
        if *self.cancel.borrow() || self.commands.is_closed() {
            return Err(DriverError::Closed);
        }
        let caps = self.kind.capabilities();
        let support = match command {
            Control::Queue(_) => caps.queue,
            Control::Steer(_) => caps.steer,
            Control::Interrupt => caps.interrupt,
        };
        match support {
            ControlSupport::Native => {
                self.commands
                    .try_send((id, command))
                    .map_err(|err| match err {
                        mpsc::error::TrySendError::Closed(_) => DriverError::Closed,
                        mpsc::error::TrySendError::Full(_) => DriverError::ControlBackpressure,
                    })?;
            }
            ControlSupport::Process => self.cancel(),
            _ => {}
        }
        Ok(support)
    }
}

struct Records {
    reader: BufReader<ChildStdout>,
    bytes: Vec<u8>,
    oversized: bool,
}

impl Records {
    async fn next(&mut self) -> io::Result<Option<Result<Value, ()>>> {
        const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
        loop {
            let chunk = self.reader.fill_buf().await?;
            let newline = chunk.iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(chunk.len(), |index| index + 1);
            if count == 0 && self.bytes.is_empty() && !self.oversized {
                return Ok(None);
            }
            self.oversized |= self.bytes.len() + count > MAX_RECORD_BYTES;
            if !self.oversized {
                self.bytes.extend_from_slice(&chunk[..count]);
            }
            self.reader.consume(count);
            if newline.is_some() || count == 0 {
                let record = if self.oversized {
                    Err(())
                } else if self.bytes.iter().all(u8::is_ascii_whitespace) {
                    self.bytes.clear();
                    continue;
                } else {
                    serde_json::from_slice(&self.bytes).map_err(|_| ())
                };
                self.bytes.clear();
                self.oversized = false;
                return Ok(Some(record));
            }
        }
    }
}

async fn drive(
    kind: NativeCliKind,
    input: Input,
    prompt: String,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    mut pending: mpsc::Receiver<(u64, Control)>,
    output: (&mpsc::Sender<DriverEvent>, &watch::Sender<bool>),
) -> io::Result<Outcome> {
    let (events, active) = output;
    let mut writes = Vec::new();
    match input {
        Input::Text => {
            writes.push(prompt.into_bytes());
        }
        Input::Rpc => {
            writes.push(rpc_bytes(json!({"id":"state","type":"get_state"})));
            writes.push(rpc_bytes(
                json!({"id":"prompt","type":"prompt","message":prompt}),
            ));
        }
        Input::Argument => {}
    }
    let rpc = matches!(input, Input::Rpc);
    let (write, mut queued) = mpsc::channel::<Vec<u8>>(16);
    let send_input = async move {
        if let Some(mut stdin) = stdin {
            for packet in writes {
                stdin.write_all(&packet).await?;
            }
            if rpc {
                while let Some(packet) = queued.recv().await {
                    stdin.write_all(&packet).await?;
                }
            }
            stdin.shutdown().await?;
        }
        Ok::<(), io::Error>(())
    };
    tokio::pin!(send_input);

    let mut records = Records {
        reader: BufReader::new(stdout),
        bytes: Vec::new(),
        oversized: false,
    };
    let mut decoder = Decoder::new(kind);
    let mut write = rpc.then_some(write);
    let mut input_finished = false;
    let mut pending_ids = std::collections::BTreeSet::new();
    let mut stats_received = false;
    let mut close_deadline = None;
    loop {
        if close_deadline.is_some() && stats_received && pending_ids.is_empty() {
            write.take();
            close_deadline = None;
        }
        tokio::select! {
            result = &mut send_input, if !input_finished => {
                input_finished = true;
                if let Err(error) = result
                    && error.kind() != io::ErrorKind::BrokenPipe
                {
                    return Err(error);
                }
            }
            record = records.next() => {
                let Some(record) = record? else { break };
                let decoded = match record {
                    Ok(record) => {
                        stats_received |= rpc && record["type"] == "response" && record["id"] == "stats";
                        decoder.decode(&record)
                    }
                    Err(()) => vec![DriverEvent::MalformedOutput],
                };
                active.send_replace(decoder.accepts_controls());
                for event in decoded {
                    if let DriverEvent::ControlAck { id, .. } = &event
                        && !pending_ids.remove(id)
                    {
                        continue;
                    }
                    let _ = events.send(event).await;
                }
                if decoder.is_terminal() && close_deadline.is_none()
                    && let Some(write) = &write
                {
                    close_deadline = Some(tokio::time::Instant::now() + Duration::from_secs(2));
                    if !input_finished {
                        let _ = write.try_send(rpc_bytes(json!({"id":"stats","type":"get_session_stats"})));
                    }
                    pending.close();
                }
            }
            _ = async {
                if let Some(deadline) = close_deadline {
                    tokio::time::sleep_until(deadline).await;
                }
            }, if close_deadline.is_some() => {
                write.take();
                close_deadline = None;
            }
            Some((id, control)) = pending.recv(), if write.is_some() && close_deadline.is_none() => {
                let Some(write) = &write else { continue };
                let packet = match control {
                    Control::Queue(message) => json!({"id":id.to_string(),"type":"follow_up","message":message}),
                    Control::Steer(message) => json!({"id":id.to_string(),"type":"steer","message":message}),
                    Control::Interrupt => json!({"id":id.to_string(),"type":"abort"}),
                };
                if !decoder.accepts_controls() || pending_ids.len() >= 15 || pending_ids.contains(&id) || write.try_send(rpc_bytes(packet)).is_err() {
                    let _ = events.send(DriverEvent::ControlAck { id, accepted: false }).await;
                } else {
                    pending_ids.insert(id);
                }
            }
        }
    }
    pending.close();
    while let Ok((id, _)) = pending.try_recv() {
        let _ = events
            .send(DriverEvent::ControlAck {
                id,
                accepted: false,
            })
            .await;
    }
    Ok(decoder.outcome())
}

fn rpc_bytes(value: Value) -> Vec<u8> {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    bytes
}
