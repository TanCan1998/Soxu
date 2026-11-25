use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("soxu-magpie-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => panic!("无法创建临时夹具目录"),
            }
        }
    }

    fn directory(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(&path).expect("创建夹具子目录");
        path
    }

    fn write(&self, relative: &str, contents: &str) {
        fs::write(self.0.join(relative), contents).expect("写入夹具文件");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn recognizes_managed_configuration_and_both_route_modes() {
    let fixture = Fixture::new();
    fixture.write(
        "library.json",
        r#"{
            "mcpHub": true, "skillRouter": true,
            "mcp": [{"name":"sample-shared"}, {"name":"sample-direct","local":true}],
            "skills": [{"name":"sample-routed"}, {"name":"sample-direct-skill","direct":true}]
        }"#,
    );
    fixture.write(
        "gateway.json",
        r#"{"pid":1,"addr":"fixture","serves":"serve"}"#,
    );

    let result = discover_environment_with(Some(&fixture.0), |_| None);
    assert_eq!(
        detect_mode_from_config_dir(&fixture.0),
        EnvironmentMode::MagpieManaged
    );
    assert_eq!(result.mode, EnvironmentMode::MagpieManaged);
    assert_eq!(result.magpie.executable, ProbeStatus::NotChecked);
    assert_eq!(result.magpie.gateway_marker, ProbeStatus::Available);
    assert_eq!(result.magpie.library, ProbeStatus::Available);
    let mcp = result.magpie.mcp_config;
    let skills = result.magpie.skill_config;
    assert_eq!(mcp.default_mode, Some(McpRouteMode::Shared));
    assert_eq!(skills.default_mode, Some(SkillRouteMode::Routed));
    assert_eq!((mcp.via_router, mcp.direct), (1, 1));
    assert_eq!((skills.via_router, skills.direct), (1, 1));
}

#[test]
fn reports_partial_and_invalid_configuration_independently() {
    let fixture = Fixture::new();
    fixture.write(
        "library.json",
        r#"{"mcpHub":"invalid","mcp":[{},{"local":true},null]}"#,
    );
    fixture.write("gateway.json", r#"{"addr":"incomplete"}"#);

    let result = discover_environment_with(Some(&fixture.0), |_| None);
    assert_eq!(result.magpie.gateway_marker, ProbeStatus::Invalid);
    assert_eq!(result.magpie.library, ProbeStatus::Available);
    let mcp = result.magpie.mcp_config;
    assert_eq!(mcp.entries, ProbeStatus::Available);
    assert_eq!(mcp.setting, ProbeStatus::Invalid);
    assert_eq!(mcp.default_mode, None);
    assert_eq!((mcp.direct, mcp.unknown), (1, 2));
    assert_eq!(result.magpie.skill_config.setting, ProbeStatus::Missing);
    assert_eq!(result.magpie.skill_config.entries, ProbeStatus::Missing);

    fixture.write("library.json", "{");
    let result = discover_environment_with(Some(&fixture.0), |_| None);
    assert_eq!(result.mode, EnvironmentMode::MagpieManaged);
    assert_eq!(result.magpie.library, ProbeStatus::Invalid);
    assert_eq!(result.magpie.mcp_config.entries, ProbeStatus::Invalid);
}

#[test]
fn uses_direct_routes_when_routers_are_disabled() {
    let fixture = Fixture::new();
    fixture.write(
        "library.json",
        r#"{"mcpHub":false,"skillRouter":false,"mcp":[{}],"skills":[{}]}"#,
    );
    let result = discover_environment_with(Some(&fixture.0), |_| None);
    assert_eq!(result.magpie.gateway_marker, ProbeStatus::Missing);
    let mcp = result.magpie.mcp_config;
    let skills = result.magpie.skill_config;
    assert_eq!(mcp.default_mode, Some(McpRouteMode::Direct));
    assert_eq!(skills.default_mode, Some(SkillRouteMode::Direct));
    assert_eq!((mcp.direct, skills.direct), (1, 1));
}

fn command_file(fixture: &Fixture, name: &str) {
    let suffix = if cfg!(windows) { ".cmd" } else { "" };
    let relative = format!("bin/{name}{suffix}");
    fixture.write(&relative, "");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(fixture.0.join(relative), fs::Permissions::from_mode(0o700))
            .expect("设置夹具的执行权限");
    }
}

#[test]
fn scans_seven_native_clis_with_separate_executable_and_config_statuses() {
    let fixture = Fixture::new();
    let bin = fixture.directory("bin");
    let xdg = fixture.directory("xdg/opencode");
    let xdg = xdg.parent().expect("夹具存在父目录");
    let pi_config = fixture.directory("pi-override");
    for directory in [".claude", ".gemini/antigravity", ".copilot", ".qwen"] {
        fixture.directory(directory);
    }
    fixture.write(".codex", "");
    for command in ["claude", "codex", "pi", "agy", "copilot", "qwen"] {
        command_file(&fixture, command);
    }
    let lookup = |name: &str| match name {
        "HOME" | "USERPROFILE" => Some(fixture.0.clone().into_os_string()),
        "XDG_CONFIG_HOME" => Some(xdg.as_os_str().to_owned()),
        "PI_CODING_AGENT_DIR" => Some(pi_config.clone().into_os_string()),
        "PATH" => Some(env::join_paths([&bin]).expect("构造夹具 PATH")),
        _ => None,
    };
    let result = discover_environment_with(None, lookup);
    assert_eq!(
        detect_mode_from_config_dir(&xdg.join("magpie")),
        EnvironmentMode::StandaloneNative
    );
    assert_eq!(result.mode, EnvironmentMode::StandaloneNative);
    assert_eq!(result.magpie.library, ProbeStatus::Missing);
    for entry in result.native_clis {
        let executable = if entry.agent == NativeAgent::Opencode {
            ProbeStatus::Missing
        } else {
            ProbeStatus::Available
        };
        let config = if entry.agent == NativeAgent::Codex {
            ProbeStatus::Invalid
        } else {
            ProbeStatus::Available
        };
        assert_eq!(entry.executable, executable, "{:?}", entry.agent);
        assert_eq!(entry.config_dir, config, "{:?}", entry.agent);
    }

    command_file(&fixture, "magpie");
    let result = discover_environment_with(None, lookup);
    assert_eq!(result.mode, EnvironmentMode::MagpieManaged);
    assert_eq!(result.magpie.library, ProbeStatus::Missing);
}

#[test]
fn classifies_read_failures_without_exposing_error_text() {
    assert_eq!(
        ProbeStatus::from_io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fixture-only-error-details",
        )),
        ProbeStatus::Unreadable
    );
}

#[test]
fn resolves_each_static_config_candidate_and_honors_overrides() {
    let candidates = [
        (NativeAgent::Claude, ".claude", Some("CLAUDE_CONFIG_DIR")),
        (NativeAgent::Codex, ".codex", Some("CODEX_HOME")),
        (NativeAgent::Pi, ".pi/agent", Some("PI_CODING_AGENT_DIR")),
        (
            NativeAgent::Opencode,
            ".config/opencode",
            Some("OPENCODE_CONFIG_DIR"),
        ),
        (NativeAgent::Agy, ".gemini/antigravity", None),
        (NativeAgent::Agy, ".antigravity", None),
        (NativeAgent::Copilot, ".copilot", Some("COPILOT_HOME")),
        (NativeAgent::Qwen, ".qwen", Some("QWEN_HOME")),
    ];
    for (agent, candidate, override_var) in candidates {
        let fixture = Fixture::new();
        fixture.directory(candidate);
        let configured = |override_dir: Option<&Path>| {
            discover_environment_with(None, |key| match key {
                "HOME" | "USERPROFILE" => Some(fixture.0.clone().into_os_string()),
                _ if Some(key) == override_var => {
                    override_dir.map(|dir| dir.as_os_str().to_owned())
                }
                _ => None,
            })
            .native_clis
            .into_iter()
            .filter(|entry| entry.config_dir == ProbeStatus::Available)
            .map(|entry| entry.agent)
            .collect::<Vec<_>>()
        };
        assert_eq!(configured(None), [agent]);
        if override_var.is_some() {
            let override_dir = fixture.0.join("alternate");
            assert!(configured(Some(&override_dir)).is_empty());
            let override_dir = fixture.directory("alternate");
            assert_eq!(configured(Some(&override_dir)), [agent]);
        }
    }
}

#[test]
fn serializes_only_allowed_presence_and_sanitized_metadata() {
    let fixture = Fixture::new();
    fixture.write(
        "library.json",
        r#"{"mcpHub":true,"mcp":[{"name":"fixture-private-name","env":{"EXAMPLE_TOKEN":"fixture-private-config"}}]}"#,
    );
    fixture.write(
        "gateway.json",
        r#"{"pid":41234,"addr":"fixture-private-address","serves":"fixture-private-role"}"#,
    );
    let result = discover_environment_with(Some(&fixture.0), |key| match key {
        "MAGPIE_ADDR" => Some(OsString::from("fixture-private-env")),
        "HOME" | "USERPROFILE" => Some(fixture.0.clone().into_os_string()),
        "PATH"
        | "XDG_CONFIG_HOME"
        | "CLAUDE_CONFIG_DIR"
        | "CODEX_HOME"
        | "PI_CODING_AGENT_DIR"
        | "OPENCODE_CONFIG_DIR"
        | "COPILOT_HOME"
        | "QWEN_HOME" => None,
        _ => panic!("请求了允许列表之外的变量"),
    });

    assert!(result.environment.magpie_addr);
    let json = serde_json::to_string(&result).expect("序列化摘要");
    let debug = format!("{result:?}");
    for output in [&json, &debug] {
        assert!(!output.contains("fixture-private"));
        assert!(!output.contains("EXAMPLE_TOKEN"));
        assert!(!output.contains("41234"));
        assert!(!output.contains("soxu-magpie-"));
    }
    assert_eq!(
        serde_json::from_str::<EnvironmentDiscovery>(&json).expect("反序列化摘要"),
        result
    );
}
