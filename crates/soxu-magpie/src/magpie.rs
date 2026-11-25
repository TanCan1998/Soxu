use crate::{McpRouteMode, ProbeStatus, SkillRouteMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;

/// `library.json` 中的默认路由及各条目的配置分布。
/// 此摘要描述配置意图，已应用路由需另查对应 Agent 配置和文件链接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSummary<Mode> {
    pub default_mode: Option<Mode>,
    pub setting: ProbeStatus,
    pub entries: ProbeStatus,
    pub via_router: usize,
    pub direct: usize,
    pub unknown: usize,
}

/// Magpie 启动元数据。网关标记代表本地登记，实时连通性须单独验证。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagpieDiscovery {
    pub executable: ProbeStatus,
    pub gateway_marker: ProbeStatus,
    pub library: ProbeStatus,
    pub mcp_config: RouteSummary<McpRouteMode>,
    pub skill_config: RouteSummary<SkillRouteMode>,
}

impl MagpieDiscovery {
    pub(crate) fn inspect(directory: Option<&Path>, executable: ProbeStatus) -> Self {
        let gateway_marker = read_metadata(directory, "gateway.json").map_or_else(
            |status| status,
            |value| {
                if value["pid"].as_u64().is_some_and(|pid| pid > 0)
                    && nonempty_string(&value["addr"])
                    && nonempty_string(&value["serves"])
                {
                    ProbeStatus::Available
                } else {
                    ProbeStatus::Invalid
                }
            },
        );
        let library = read_metadata(directory, "library.json");
        let library_status = library
            .as_ref()
            .map_or_else(|status| *status, |_| ProbeStatus::Available);
        let library_value = library.as_ref().ok();

        Self {
            executable,
            gateway_marker,
            library: library_status,
            mcp_config: routes(
                library_value,
                library_status,
                ("mcpHub", "mcp", "local"),
                (McpRouteMode::Shared, McpRouteMode::Direct),
            ),
            skill_config: routes(
                library_value,
                library_status,
                ("skillRouter", "skills", "direct"),
                (SkillRouteMode::Routed, SkillRouteMode::Direct),
            ),
        }
    }
}

fn nonempty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty())
}

fn read_metadata(directory: Option<&Path>, filename: &str) -> Result<Value, ProbeStatus> {
    let path = directory.ok_or(ProbeStatus::NotChecked)?.join(filename);
    if !fs::metadata(&path).map_err(ProbeStatus::from_io)?.is_file() {
        return Err(ProbeStatus::Invalid);
    }
    let file = File::open(path).map_err(ProbeStatus::from_io)?;
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(ProbeStatus::from_io)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(ProbeStatus::Invalid);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| ProbeStatus::Invalid)?;
    value
        .is_object()
        .then_some(value)
        .ok_or(ProbeStatus::Invalid)
}

fn routes<Mode>(
    library: Option<&Value>,
    library_status: ProbeStatus,
    (toggle_key, entries_key, direct_key): (&str, &str, &str),
    (routed_mode, direct_mode): (Mode, Mode),
) -> RouteSummary<Mode> {
    let setting = library.and_then(|value| value.get(toggle_key));
    let toggle = setting.and_then(Value::as_bool);
    let mut summary = RouteSummary {
        default_mode: toggle.map(|enabled| if enabled { routed_mode } else { direct_mode }),
        setting: match (library, setting, toggle) {
            (_, _, Some(_)) => ProbeStatus::Available,
            (_, Some(_), None) => ProbeStatus::Invalid,
            (Some(_), None, None) => ProbeStatus::Missing,
            (None, None, None) => library_status,
        },
        entries: library_status,
        via_router: 0,
        direct: 0,
        unknown: 0,
    };
    let Some(library) = library else {
        return summary;
    };
    let Some(entries) = library.get(entries_key) else {
        summary.entries = ProbeStatus::Missing;
        return summary;
    };
    let Some(entries) = entries.as_array() else {
        summary.entries = ProbeStatus::Invalid;
        return summary;
    };

    for entry in entries {
        let Some(item) = entry.as_object() else {
            summary.unknown += 1;
            continue;
        };
        match (toggle, item.get(direct_key)) {
            (Some(false), _) | (_, Some(Value::Bool(true))) => summary.direct += 1,
            (Some(true), None | Some(Value::Bool(false))) => summary.via_router += 1,
            _ => summary.unknown += 1,
        }
    }
    summary
}
