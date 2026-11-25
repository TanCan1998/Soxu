use std::ffi::OsString;
use std::fs::{self, File, Metadata};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::{NativeCliKind, SessionMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub kind: NativeCliKind,
    pub id: String,
    pub cwd: Option<PathBuf>,
    pub started_at: Option<String>,
    pub modified: Option<SystemTime>,
    pub source: PathBuf,
}

impl HistoryEntry {
    pub fn resume(&self) -> SessionMode {
        SessionMode::Resume(self.selector())
    }

    pub fn fork(&self) -> Option<SessionMode> {
        self.kind
            .capabilities()
            .fork
            .then(|| SessionMode::Fork(self.selector()))
    }

    fn selector(&self) -> OsString {
        if self.kind == NativeCliKind::Pi {
            self.source.as_os_str().to_owned()
        } else {
            self.id.clone().into()
        }
    }
}

#[derive(Debug, Default)]
pub struct HistoryListing {
    pub entries: Vec<HistoryEntry>,
    pub skipped: usize,
    pub limited: bool,
}

/// 只读扫描 Daemon 指定的原生历史目录。仅提取会话标识与定位信息。
///
/// 缺少目录返回空结果，未实现的格式返回 `Unsupported`。在 Tokio 服务中应通过
/// `spawn_blocking` 调用。每次扫描最多查看 20,000 个目录项，文件探测最多读取 256 KiB。
pub fn discover_history(kind: NativeCliKind, root: &Path) -> io::Result<HistoryListing> {
    if !kind.capabilities().discover_history {
        return Err(io::ErrorKind::Unsupported.into());
    }
    if !root.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    match fs::symlink_metadata(root) {
        Ok(meta) if meta.is_dir() && !is_link(&meta) => {}
        Ok(_) => return Err(io::ErrorKind::InvalidInput.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(HistoryListing::default());
        }
        Err(error) => return Err(error),
    }
    let mut visited = 0;
    let mut listing = HistoryListing::default();
    visit(kind, root, 0, &mut visited, &mut listing)?;
    listing.entries.sort_unstable_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.source.cmp(&right.source))
    });
    let mut seen = std::collections::HashSet::new();
    listing
        .entries
        .retain(|entry| seen.insert(entry.id.clone()));
    Ok(listing)
}

fn visit(
    kind: NativeCliKind,
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    listing: &mut HistoryListing,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        *visited += 1;
        if *visited > 20_000 {
            listing.limited = true;
            break;
        }
        let Ok(entry) = entry else {
            listing.skipped += 1;
            continue;
        };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            listing.skipped += 1;
            continue;
        };
        if is_link(&meta) {
            continue;
        }
        if meta.is_dir() {
            if depth >= 8 {
                listing.limited = true;
            } else if visit(kind, &path, depth + 1, visited, listing).is_err() {
                listing.skipped += 1;
            }
        } else if meta.is_file() && candidate(kind, &path) {
            match read_entry(kind, path, &meta) {
                Some(entry) => listing.entries.push(entry),
                None => listing.skipped += 1,
            }
        }
    }
    Ok(())
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn candidate(kind: NativeCliKind, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    use NativeCliKind::{ClaudeCode, Codex, Copilot, Pi, QwenCode};
    match kind {
        Codex => name.starts_with("rollout-") && name.ends_with(".jsonl"),
        Pi => name.ends_with(".jsonl"),
        ClaudeCode => name.strip_suffix(".jsonl").is_some_and(uuid_like),
        QwenCode => {
            name.strip_suffix(".jsonl").is_some_and(uuid_like)
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|parent| parent == "chats")
        }
        Copilot => name == "events.jsonl",
        _ => false,
    }
}

fn read_entry(kind: NativeCliKind, path: PathBuf, meta: &Metadata) -> Option<HistoryEntry> {
    let mut reader = BufReader::new(File::open(&path).ok()?.take(256 * 1024));
    let mut bytes = Vec::new();
    for _ in 0..64 {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes).ok()? == 0 {
            return None;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if let Some((id, cwd, time)) = metadata(kind, &path, &value) {
            return Some(HistoryEntry {
                kind,
                id: id.to_owned(),
                cwd: cwd
                    .as_str()
                    .map(PathBuf::from)
                    .filter(|cwd| cwd.is_absolute()),
                started_at: time.as_str().map(str::to_owned),
                modified: meta.modified().ok(),
                source: path,
            });
        }
    }
    None
}

fn metadata<'a>(
    kind: NativeCliKind,
    path: &Path,
    value: &'a Value,
) -> Option<(&'a str, &'a Value, &'a Value)> {
    use NativeCliKind::{ClaudeCode, Codex, Copilot, Pi, QwenCode};
    let (id, cwd, time) = match kind {
        Codex if value["type"] == "session_meta" => {
            let payload = &value["payload"];
            (&payload["id"], &payload["cwd"], &payload["timestamp"])
        }
        Pi if value["type"] == "session" => (&value["id"], &value["cwd"], &value["timestamp"]),
        ClaudeCode
            if matches!(value["type"].as_str(), Some("user" | "assistant"))
                && value["isSidechain"] != true =>
        {
            (&value["sessionId"], &value["cwd"], &value["timestamp"])
        }
        QwenCode if value["uuid"].is_string() => {
            (&value["sessionId"], &value["cwd"], &value["timestamp"])
        }
        Copilot if value["type"] == "session.start" => {
            let context = &value["data"]["context"];
            (
                &value["data"]["sessionId"],
                &context["cwd"],
                &value["timestamp"],
            )
        }
        _ => return None,
    };
    let id = id.as_str().filter(|id| uuid_like(id))?;
    match kind {
        ClaudeCode | QwenCode if path.file_stem()?.to_str()? != id => None,
        Copilot if path.parent()?.file_name()?.to_str()? != id => None,
        _ => Some((id, cwd, time)),
    }
}

fn uuid_like(value: &str) -> bool {
    if value.len() == 32 {
        return value.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
