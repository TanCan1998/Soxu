//! 调度定义持久化（单 JSON 文件 + 原子替换）。
//!
//! 守护进程的产品状态库（`store.rs`）不在本任务的写入白名单内，因此调度定义
//! 由调度域自持一个受控文件：体积与条目上限都与执行器的
//! [`crate::schedules::MAX_SCHEDULES_PER_WORKSPACE`] 对齐，避免无界增长。
//!
//! 写入流程：序列化 → 体积复核 → 写临时文件 → `rename` 原子替换。读取失败、
//! 版本未知、条目重复一律 fail-closed 返回可执行错误，不做「尽量恢复」的降级。

use crate::schedules::executor::StoredSchedule;
use soxu_protocol::{ActionableError, ErrorCode, ScheduleId};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 调度定义文件体积上限（4 MiB；256 条定义远低于该值）。
pub const SCHEDULE_STORE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 当前文件格式版本。
const STORE_VERSION: u32 = 1;

/// 调度定义存储。
#[derive(Debug, Clone)]
pub struct ScheduleStore {
    path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ScheduleFile {
    version: u32,
    schedules: Vec<StoredSchedule>,
}

impl ScheduleStore {
    /// 绑定存储路径（此时不读写磁盘）。
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 存储路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 读取全部定义；文件不存在时返回空表。
    pub fn load(&self) -> Result<BTreeMap<ScheduleId, StoredSchedule>, ActionableError> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(store_error("读取", &self.path, &error.to_string())),
        };
        if metadata.len() > SCHEDULE_STORE_MAX_BYTES {
            return Err(ActionableError::new(
                ErrorCode::WorkspaceDamaged,
                format!("调度定义文件超过 {SCHEDULE_STORE_MAX_BYTES} 字节上限"),
                "守护进程拒绝加载该文件，既有内存中的调度定义未被覆盖",
                "请检查是否被外部写入放大，或删除该文件后重新创建调度",
            ));
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| store_error("读取", &self.path, &error.to_string()))?;
        let file: ScheduleFile = serde_json::from_slice(&bytes).map_err(|error| {
            ActionableError::new(
                ErrorCode::WorkspaceDamaged,
                "调度定义文件不是合法的 JSON 结构".to_owned(),
                "守护进程拒绝加载该文件，没有任何调度会被触发",
                "请修复或删除该文件后重新创建调度；不要手工拼接半份数据",
            )
            .with_diagnostic(format!("path={}, error={error}", self.path.display()))
        })?;
        if file.version != STORE_VERSION {
            return Err(ActionableError::new(
                ErrorCode::Unsupported,
                format!("调度定义文件版本 {} 不受支持", file.version),
                "守护进程拒绝加载该文件，没有任何调度会被触发",
                format!("请使用支持版本 {STORE_VERSION} 的守护进程，或迁移该文件"),
            ));
        }
        let mut schedules = BTreeMap::new();
        for schedule in file.schedules {
            let id = schedule.summary.schedule_id.clone();
            if schedules.insert(id.clone(), schedule).is_some() {
                return Err(ActionableError::new(
                    ErrorCode::WorkspaceDamaged,
                    format!("调度定义文件出现重复标识 `{}`", id.as_str()),
                    "守护进程拒绝加载该文件，没有任何调度会被触发",
                    "请删除重复条目后重新启动守护进程",
                ));
            }
        }
        Ok(schedules)
    }

    /// 原子写回全部定义。
    pub fn save(
        &self,
        schedules: &BTreeMap<ScheduleId, StoredSchedule>,
    ) -> Result<(), ActionableError> {
        let file = ScheduleFile {
            version: STORE_VERSION,
            schedules: schedules.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|error| {
            ActionableError::new(
                ErrorCode::InternalError,
                "调度定义序列化失败",
                "本次改动未被持久化，内存中的定义保持最新",
                "请重试；若持续失败请上报诊断",
            )
            .with_diagnostic(error.to_string())
        })?;
        if bytes.len() as u64 > SCHEDULE_STORE_MAX_BYTES {
            return Err(ActionableError::new(
                ErrorCode::Conflict,
                format!("调度定义超过 {SCHEDULE_STORE_MAX_BYTES} 字节上限，拒绝写入"),
                "本次改动未被持久化，调度器不会按新定义触发",
                "请删除不再需要的调度定义后重试",
            ));
        }
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .map_err(|error| store_error("创建目录", parent, &error.to_string()))?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, &bytes)
            .map_err(|error| store_error("写入临时文件", &temporary, &error.to_string()))?;
        fs::rename(&temporary, &self.path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            store_error("替换调度定义文件", &self.path, &error.to_string())
        })
    }
}

fn store_error(action: &str, path: &Path, detail: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::InternalError,
        format!("{action}调度定义文件失败"),
        "本次改动的持久化结论不明确，内存中的调度定义保持最新",
        "请检查数据目录可写性与磁盘空间后重试",
    )
    .with_diagnostic(format!("path={}, detail={detail}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use soxu_protocol::{
        CronExpression, IdempotencyKey, ScheduleCadence, ScheduleName, ScheduleRetryPolicy,
        ScheduleState, ScheduleSummary, ScheduleTrigger, ScheduleValidityWindow,
    };
    use tempfile::TempDir;

    fn moment(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("合法时间戳")
    }

    fn stored(id: &str) -> StoredSchedule {
        let key = IdempotencyKey::try_from(format!("key-{id}")).expect("合法幂等键");
        StoredSchedule {
            summary: ScheduleSummary {
                schedule_id: ScheduleId::try_from(id.to_owned()).expect("合法调度标识"),
                workspace_id: "wks_test".to_owned(),
                name: ScheduleName::try_from("每日巡检".to_owned()).expect("合法名称"),
                cadence: ScheduleCadence::Cron {
                    expression: CronExpression::try_from("0 3 * * *".to_owned())
                        .expect("合法表达式"),
                    timezone: None,
                },
                trigger: ScheduleTrigger {
                    agent_id: "agent-1".to_owned(),
                    model_id: None,
                    permission: None,
                    text: "巡检".to_owned(),
                },
                validity: ScheduleValidityWindow::default(),
                retry: ScheduleRetryPolicy::default(),
                state: ScheduleState::Active,
                next_run_at: Some(moment(1_800_000_000)),
                last_run_at: None,
                created_at: moment(1_799_000_000),
                updated_at: moment(1_799_000_000),
            },
            paused: false,
            last_result: None,
            attempts: 0,
            pending_retry: None,
            idempotency_key: key,
        }
    }

    #[test]
    fn missing_file_loads_as_empty() {
        let temp = TempDir::new().expect("临时目录");
        let store = ScheduleStore::new(temp.path().join("schedules.json"));
        assert!(store.load().expect("缺文件应返回空表").is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let temp = TempDir::new().expect("临时目录");
        let store = ScheduleStore::new(temp.path().join("nested").join("schedules.json"));
        let mut schedules = BTreeMap::new();
        let entry = stored("sch_a");
        schedules.insert(entry.summary.schedule_id.clone(), entry.clone());
        store.save(&schedules).expect("写入应成功");

        let loaded = store.load().expect("读取应成功");
        assert_eq!(loaded, schedules);
        assert!(
            !store.path().with_extension("json.tmp").exists(),
            "临时文件必须被 rename 消费掉"
        );
    }

    #[test]
    fn corrupt_file_is_rejected_without_partial_recovery() {
        let temp = TempDir::new().expect("临时目录");
        let path = temp.path().join("schedules.json");
        let store = ScheduleStore::new(&path);
        fs::write(&path, b"{ not json").expect("写入损坏夹具");
        let error = store.load().expect_err("损坏文件必须被拒绝");
        assert_eq!(error.code, ErrorCode::WorkspaceDamaged);
        assert!(!error.remediation_zh.is_empty());
    }

    #[test]
    fn unknown_version_is_rejected() {
        let temp = TempDir::new().expect("临时目录");
        let path = temp.path().join("schedules.json");
        let store = ScheduleStore::new(&path);
        fs::write(&path, br#"{"version":99,"schedules":[]}"#).expect("写入版本夹具");
        let error = store.load().expect_err("未知版本必须被拒绝");
        assert_eq!(error.code, ErrorCode::Unsupported);
    }

    #[test]
    fn duplicate_identifiers_are_rejected() {
        let temp = TempDir::new().expect("临时目录");
        let path = temp.path().join("schedules.json");
        let store = ScheduleStore::new(&path);
        let entry = stored("sch_dup");
        let body = serde_json::to_vec(&ScheduleFile {
            version: STORE_VERSION,
            schedules: vec![entry.clone(), entry],
        })
        .expect("序列化夹具");
        fs::write(&path, body).expect("写入重复夹具");
        let error = store.load().expect_err("重复标识必须被拒绝");
        assert_eq!(error.code, ErrorCode::WorkspaceDamaged);
        assert!(error.summary_zh.contains("重复"));
    }
}
