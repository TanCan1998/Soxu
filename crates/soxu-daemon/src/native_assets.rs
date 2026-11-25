use std::backtrace::Backtrace;
use std::fs::{self, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

const MAX_ASSET_BYTES: usize = 1024 * 1024;

pub struct NativeAssetGuard {
    root: PathBuf,
    assets: Vec<AssetFingerprint>,
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

fn checked_directory(directory: &Path) -> Result<(), NativeAssetError> {
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
        Ok(Self { root, assets })
    }

    pub fn verify(&self) -> Result<(), NativeAssetError> {
        checked_directory(&self.root)?;
        for asset in &self.assets {
            let path = self.root.join(&asset.path);
            checked_directory(path.parent().ok_or_else(|| rejected("资产缺少父目录"))?)?;
            let metadata = fs::symlink_metadata(&path)?;
            if is_link(&metadata) || !metadata.is_file() || metadata.len() != asset.bytes as u64 {
                return Err(rejected("资产缺失、被替换或大小改变"));
            }
            let file = fs::File::open(path)?;
            let mut content = Vec::with_capacity(asset.bytes);
            file.take(MAX_ASSET_BYTES as u64 + 1)
                .read_to_end(&mut content)?;
            if content.len() != asset.bytes
                || <[u8; 32]>::from(Sha256::digest(&content)) != asset.sha256
            {
                return Err(rejected("资产内容与可信声明不符"));
            }
        }
        Ok(())
    }

    pub fn verify_launch_directory(&self, directory: &Path) -> Result<(), NativeAssetError> {
        self.verify()?;
        checked_directory(directory)?;
        let directory = directory.canonicalize()?;
        if !directory.starts_with(&self.root) {
            return Err(rejected("启动目录不在可信资产根内"));
        }
        for ancestor in directory.ancestors() {
            match fs::symlink_metadata(ancestor.join(".agents")) {
                Ok(_) => return Err(rejected("启动目录或祖先包含项目自定义资产")),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            if ancestor == self.root {
                break;
            }
        }
        Ok(())
    }
}
