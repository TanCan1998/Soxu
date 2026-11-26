use std::backtrace::Backtrace;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

const MAX_ASSET_BYTES: usize = 1024 * 1024;

pub struct NativeAssetGuard {
    root: PathBuf,
    assets: Vec<AssetFingerprint>,
    private: bool,
    #[cfg(unix)]
    owner: Option<u32>,
}

struct AssetFingerprint {
    path: PathBuf,
    bytes: usize,
    sha256: [u8; 32],
}

#[derive(Debug)]
pub struct NativeAssetError {
    reason: &'static str,
    cause: Option<io::Error>,
    backtrace: Backtrace,
}

impl std::fmt::Display for NativeAssetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "原生启动资产校验失败：{}", self.reason)?;
        if let Some(cause) = &self.cause {
            write!(formatter, "（{:?}）", cause.kind())?;
        }
        if self.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(formatter, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for NativeAssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| cause as _)
    }
}

impl From<io::Error> for NativeAssetError {
    fn from(cause: io::Error) -> Self {
        Self {
            reason: "资产无法读取",
            cause: Some(cause),
            backtrace: Backtrace::capture(),
        }
    }
}

fn rejected(reason: &'static str) -> NativeAssetError {
    NativeAssetError {
        reason,
        cause: None,
        backtrace: Backtrace::capture(),
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(crate) fn checked_directory(directory: &Path) -> Result<(), NativeAssetError> {
    if !directory.is_absolute()
        || directory
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(rejected("目录必须为不含跳转段的绝对路径"));
    }
    for ancestor in directory.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(rejected("目录或祖先不是普通目录"));
        }
    }
    Ok(())
}

impl NativeAssetGuard {
    pub fn new(
        root: PathBuf,
        expected: impl IntoIterator<Item = (PathBuf, Vec<u8>)>,
    ) -> Result<Self, NativeAssetError> {
        checked_directory(&root)?;
        let root = root.canonicalize()?;
        let mut assets = Vec::<AssetFingerprint>::new();
        for (path, content) in expected {
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
                || content.len() > MAX_ASSET_BYTES
                || assets.iter().any(|asset| asset.path == path)
            {
                return Err(rejected("资产身份、大小或重复声明无效"));
            }
            assets.push(AssetFingerprint {
                path,
                bytes: content.len(),
                sha256: Sha256::digest(&content).into(),
            });
        }
        if assets.is_empty() {
            return Err(rejected("必须声明可信资产"));
        }
        Ok(Self {
            root,
            assets,
            private: false,
            #[cfg(unix)]
            owner: None,
        })
    }

    pub(crate) fn protect(mut self) -> Result<Self, NativeAssetError> {
        self.private = true;
        #[cfg(unix)]
        {
            let executable = ["/usr/bin/id", "/bin/id"]
                .into_iter()
                .find(|path| Path::new(path).is_file())
                .ok_or_else(|| rejected("找不到系统用户身份查询程序"))?;
            let output = std::process::Command::new(executable)
                .arg("-u")
                .env_clear()
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()?;
            self.owner = output
                .status
                .success()
                .then_some(output.stdout)
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|uid| uid.trim().parse().ok());
            if self.owner.is_none() {
                return Err(rejected("无法确认守护进程用户身份"));
            }
        }
        self.verify()?;
        Ok(self)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn is_private(&self) -> bool {
        self.private
    }

    pub fn verify(&self) -> Result<(), NativeAssetError> {
        self.open_verified_assets().map(|_| ())
    }

    fn open_verified_assets(&self) -> Result<Vec<File>, NativeAssetError> {
        checked_directory(&self.root)?;
        let mut files = Vec::with_capacity(self.assets.len());
        for asset in &self.assets {
            let path = self.root.join(&asset.path);
            let parent = path.parent().ok_or_else(|| rejected("资产缺少父目录"))?;
            self.verify_private_directory(parent)?;
            let metadata = fs::symlink_metadata(&path)?;
            if is_link(&metadata) || !metadata.is_file() || metadata.len() != asset.bytes as u64 {
                return Err(rejected("资产缺失、被替换或大小改变"));
            }
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            if self.private {
                use std::os::windows::fs::OpenOptionsExt as _;
                options.share_mode(1);
            }
            let mut file = options.open(&path)?;
            let opened = file.metadata()?;
            self.verify_private_permissions(&opened)?;
            if !opened.is_file() || is_link(&opened) || !same_asset(&metadata, &opened) {
                return Err(rejected("打开的资产与路径检查不一致"));
            }
            let mut content = Vec::with_capacity(asset.bytes);
            (&mut file)
                .take(MAX_ASSET_BYTES as u64 + 1)
                .read_to_end(&mut content)?;
            if content.len() != asset.bytes
                || <[u8; 32]>::from(Sha256::digest(&content)) != asset.sha256
            {
                return Err(rejected("资产内容与可信声明不符"));
            }
            self.verify_private_directory(parent)?;
            let current = fs::symlink_metadata(&path)?;
            if is_link(&current) || !same_asset(&opened, &current) {
                return Err(rejected("读取期间资产路径改变"));
            }
            if self.private {
                files.push(file);
            }
        }
        Ok(files)
    }

    pub(crate) fn verify_private_directory(
        &self,
        directory: &Path,
    ) -> Result<(), NativeAssetError> {
        checked_directory(directory)?;
        if self.private {
            for path in directory
                .ancestors()
                .take_while(|path| path.starts_with(&self.root))
            {
                self.verify_private_permissions(&fs::symlink_metadata(path)?)?;
            }
        }
        Ok(())
    }

    fn verify_private_permissions(&self, metadata: &Metadata) -> Result<(), NativeAssetError> {
        #[cfg(unix)]
        if let Some(owner) = self.owner {
            use std::os::unix::fs::MetadataExt as _;
            if metadata.uid() != owner || metadata.mode() & 0o077 != 0 {
                return Err(rejected("隔离资产须由守护进程用户独占访问"));
            }
        }
        #[cfg(not(unix))]
        let _ = metadata;
        Ok(())
    }

    pub fn verify_launch_directory(&self, directory: &Path) -> Result<(), NativeAssetError> {
        self.hold_launch_assets(directory).map(|_| ())
    }

    pub(crate) fn hold_launch_assets(
        &self,
        directory: &Path,
    ) -> Result<Vec<File>, NativeAssetError> {
        let files = self.open_verified_assets()?;
        self.verify_private_directory(directory)?;
        let directory = directory.canonicalize()?;
        if !directory.starts_with(&self.root) {
            return Err(rejected("启动目录不在可信资产根内"));
        }
        for ancestor in directory.ancestors() {
            let names: &[&str] = if !self.private {
                &[".agents"]
            } else if ancestor.starts_with(&self.root) {
                &[".agents", ".git", "AGENTS.md", "GEMINI.md"]
            } else {
                &[".git", "AGENTS.md", "GEMINI.md"]
            };
            for name in names {
                match fs::symlink_metadata(ancestor.join(name)) {
                    Ok(_) => return Err(rejected("启动目录或祖先包含项目自定义资产")),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if !self.private && ancestor == self.root {
                break;
            }
        }
        Ok(files)
    }
}

fn same_asset(expected: &Metadata, actual: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if expected.dev() != actual.dev() || expected.ino() != actual.ino() {
            return false;
        }
    }
    expected.len() == actual.len()
        && expected.modified().ok() == actual.modified().ok()
        && expected.created().ok() == actual.created().ok()
}
