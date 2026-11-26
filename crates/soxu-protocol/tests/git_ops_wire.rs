use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use soxu_protocol::*;

const TIME: &str = "2026-11-26T12:00:00Z";
const PARENT: &str = "1111111111111111111111111111111111111111";
const COMMIT: &str = "2222222222222222222222222222222222222222";
const BASE_TREE: &str = "3333333333333333333333333333333333333333";
const CHECKPOINT_TREE: &str = "4444444444444444444444444444444444444444";

fn assert_wire<T: DeserializeOwned + Serialize>(expected: Value) -> T {
    let decoded: T = serde_json::from_value(expected.clone()).expect("解析约定的 JSON");
    assert_eq!(
        serde_json::to_value(&decoded).expect("编码协议值"),
        expected
    );
    decoded
}

macro_rules! assert_tags {
    ($type:ty, $($variant:ident => $tag:literal),+ $(,)?) => {
        $(assert_eq!(assert_wire::<$type>(json!($tag)), <$type>::$variant);)+
    };
}

#[test]
fn stable_error_and_stage_tags() {
    assert_tags!(ErrorCode,
        Unsupported => "unsupported", IdentityChanged => "identity_changed",
        PolicyDenied => "policy_denied", SafetyIncomplete => "safety_incomplete",
        PartialNeedsRecovery => "partial_needs_recovery",
        RefPublishedIndexPending => "ref_published_index_pending",
    );
    assert_tags!(RewindStage,
        Planned => "planned", AwaitingApproval => "awaiting_approval",
        Revalidating => "revalidating", CapturingSafety => "capturing_safety",
        SafetyDurable => "safety_durable", Applying => "applying", Verifying => "verifying",
        Completed => "completed", Rejected => "rejected", Invalidated => "invalidated",
        FailedBeforeApply => "failed_before_apply", PartialNeedsRecovery => "partial_needs_recovery",
    );
    assert_tags!(CommitStage,
        Planned => "planned", AwaitingApproval => "awaiting_approval",
        Revalidating => "revalidating", StagingPrivateTree => "staging_private_tree",
        PublishReady => "publish_ready", RefPublished => "ref_published", IndexInstalled => "index_installed",
        Completed => "completed", Rejected => "rejected", Invalidated => "invalidated",
        FailedBeforePublish => "failed_before_publish", PartialNeedsRecovery => "partial_needs_recovery",
    );
    assert_tags!(GitOperationKind,
        Checkpoint => "checkpoint", Rewind => "rewind", StructuredCommit => "structured_commit",
    );
    assert_eq!(
        assert_wire::<GitOperationStage>(json!({"kind":"checkpoint","stage":"complete"})).kind(),
        GitOperationKind::Checkpoint
    );
    assert_eq!(
        assert_wire::<GitOperationStage>(json!({"kind":"rewind","stage":"safety_durable"})).kind(),
        GitOperationKind::Rewind
    );
    assert_eq!(
        assert_wire::<GitOperationStage>(
            json!({"kind":"structured_commit","stage":"ref_published"})
        )
        .kind(),
        GitOperationKind::StructuredCommit
    );
    assert!(
        serde_json::from_value::<GitOperationStage>(
            json!({"kind":"rewind","stage":"ref_published"})
        )
        .is_err()
    );
}

#[test]
fn stable_coverage_path_and_policy_tags() {
    assert_tags!(CheckpointState, Complete => "complete", Incomplete => "incomplete", Unavailable => "unavailable");
    assert_tags!(CheckpointOmissionReason,
        Ignored => "ignored", PolicyDenied => "policy_denied", SizeLimit => "size_limit",
        UnsupportedEntry => "unsupported_entry", PathEncoding => "path_encoding", ReadFailed => "read_failed",
        BudgetExceeded => "budget_exceeded",
    );
    assert_tags!(CheckpointDiffIncompleteReason,
        EndpointCoverage => "endpoint_coverage", DiffBudget => "diff_budget",
        EndpointCoverageAndDiffBudget => "endpoint_coverage_and_diff_budget",
    );
    assert_tags!(GitPathChangeKind,
        Added => "added", Deleted => "deleted", Modified => "modified", Renamed => "renamed", Unchanged => "unchanged",
    );
    assert_tags!(SafetyProtectionState, Pending => "pending", Durable => "durable", Incomplete => "incomplete");
    assert_tags!(SafetyVerificationState, Pending => "pending", Verified => "verified", Failed => "failed");
    assert_wire::<SafetyRetentionPolicy>(json!({"kind":"until","retain_until":TIME}));
    assert_tags!(CommitContentSource, Worktree => "worktree", Index => "index", Deletion => "deletion");
    assert_tags!(CommitPathRisk, SizeLimit => "size_limit", Binary => "binary", SensitiveContent => "sensitive_content");
    assert_tags!(CommitHook, PreCommit => "pre_commit", PrepareCommitMsg => "prepare_commit_msg", CommitMsg => "commit_msg");
}

#[test]
fn fixed_endpoints_reject_git_references_and_abbreviations() {
    assert_eq!(
        assert_wire::<GitObjectId>(json!(BASE_TREE)).as_str(),
        BASE_TREE
    );
    assert_wire::<GitObjectId>(json!("a".repeat(64)));
    for invalid in ["refs/soxu/private/checkpoint", "HEAD^{tree}", "123abcd"] {
        assert!(
            serde_json::from_value::<CheckpointBaseline>(json!({
                "kind": "authorized_base_tree", "tree_oid": invalid
            }))
            .is_err(),
            "{invalid}"
        );
    }
    assert!(
        serde_json::from_value::<CheckpointBaseline>(json!({
            "kind": "ref", "ref": "refs/soxu/private/checkpoint"
        }))
        .is_err()
    );
}

fn coverage() -> Value {
    json!({"tracked_paths":12, "untracked_paths":2, "omitted_paths":1, "manifest_digest":"coverage-1"})
}

fn omissions() -> Value {
    json!([{"path": null, "reason": "policy_denied"}])
}

fn checkpoint() -> Value {
    json!({
        "workspace_id":"ws-1", "session_id":"session-1", "checkpoint_id":"cp-1",
        "parent_oid":PARENT, "worktree_tree_oid":CHECKPOINT_TREE, "commit_oid":COMMIT,
        "turn_index":1, "changed_files":3, "coverage":coverage(), "omissions":omissions(),
        "state":"incomplete", "created_at":TIME
    })
}

fn rename() -> Value {
    json!({"kind":"renamed", "path":"src/new.rs", "previous_path":"src/old.rs"})
}

fn operation() -> Value {
    json!({
        "operation_id":"op-1", "workspace_id":"ws-1", "session_id":null,
        "stage":{"kind":"structured_commit", "stage":"partial_needs_recovery"},
        "error_code":"ref_published_index_pending",
        "published_commit":{"branch_ref":"refs/heads/topic", "commit_oid":COMMIT, "paths":["src/old.rs", "src/new.rs"]},
        "applied":{"worktree_paths":[], "index_paths":[]},
        "pending_recovery":{"worktree_paths":[], "index_paths":["src/old.rs", "src/new.rs"]},
        "unknown":{"worktree_paths":[], "index_paths":[]},
        "safety":null, "ref_published_index_pending":true, "summary":"已发布，待同步暂存区"
    })
}

fn command(method: &str, params: Value) {
    assert_wire::<RpcRequest>(
        json!({"request_id":"r-1", "command":{"method":method, "params":params}}),
    );
}

fn result(kind: &str, data: Value) {
    assert_wire::<RpcResponse>(
        json!({"request_id":"r-1", "ok":true, "result":{"kind":kind, "data":data}, "error":null}),
    );
}

#[test]
fn checkpoint_listing_fixed_diff_and_event() {
    let from = json!({"kind":"authorized_base_tree", "tree_oid":BASE_TREE});
    let to = json!({"kind":"checkpoint", "checkpoint_id":"cp-1"});
    command(
        "list_checkpoints",
        json!({"workspace_id":"ws-1", "session_id":"session-1"}),
    );
    result(
        "checkpoints_listed",
        json!({"workspace_id":"ws-1", "session_id":"session-1", "checkpoints":[checkpoint()]}),
    );
    command(
        "compare_checkpoints",
        json!({"workspace_id":"ws-1", "session_id":"session-1", "from":from, "to":to}),
    );
    result(
        "checkpoints_compared",
        json!({
            "workspace_id":"ws-1", "session_id":"session-1",
            "from":{
                "baseline":from, "tree_oid":BASE_TREE, "commit_oid":PARENT,
                "coverage":{"tracked_paths":12,"untracked_paths":0,"omitted_paths":0,"manifest_digest":null},
                "omissions":[], "state":"complete"
            },
            "to":{
                "baseline":to, "tree_oid":CHECKPOINT_TREE, "commit_oid":COMMIT,
                "coverage":coverage(), "omissions":omissions(), "state":"incomplete"
            },
            "coverage":coverage(), "omissions":omissions(), "incomplete_reason":"endpoint_coverage",
            "files":[{
                "change":rename(), "binary":false,"added_lines":2,"deleted_lines":1,
                "hunks":[{"old_start":1,"old_lines":1,"new_start":1,"new_lines":2}]
            }]
        }),
    );
    assert_wire::<EventEnvelope<DaemonEvent>>(json!({
        "protocol_version":1, "seq":9,"timestamp":TIME,
        "payload":{"event":"checkpoint_recorded", "data":checkpoint()}
    }));
}

#[test]
fn rewind_preflight_and_two_independent_approvals() {
    let worktree =
        json!({"action":"restore_worktree", "target":{"kind":"checkpoint","checkpoint_id":"cp-1"}});
    let index = json!({"action":"restore_index", "safety_id":"safe-0"});
    let mut worktree_scope = worktree.clone();
    worktree_scope["paths"] = json!(["src/old.rs", "src/new.rs"]);
    let mut index_scope = index.clone();
    index_scope["paths"] = json!(["src/lib.rs"]);
    let changes =
        json!([{"change":rename(), "current_content_digest":"before", "target_blob_oid":COMMIT}]);
    command(
        "preview_rewind",
        json!({"workspace_id":"ws-1", "session_id":"session-1", "actions":[worktree,index]}),
    );
    result(
        "rewind_previewed",
        json!({
            "operation_id":"op-1", "workspace_id":"ws-1", "session_id":"session-1", "actions":[worktree_scope,index_scope],
            "worktree_changes":changes, "index_changes":[], "estimated_safety_bytes":4096,
            "blockers":[{"code":"conflict", "path":"src/new.rs", "summary":"路径冲突"}],
            "plan_digest":"digest-1", "expires_at":TIME
        }),
    );
    assert_wire::<RewindTargetAction>(json!({
        "action":"restore_worktree", "target":{"kind":"safety","safety_id":"safe-0"}
    }));
    for actions in [
        json!([worktree_scope]),
        json!([index_scope]),
        json!([worktree_scope, index_scope]),
    ] {
        command(
            "execute_rewind",
            json!({
                "operation_id":"op-1", "workspace_id":"ws-1", "session_id":"session-1",
                "approved_actions":actions, "plan_digest":"digest-1", "idempotency_key":"idem-1", "expires_at":TIME
            }),
        );
    }

    let rewind = json!({
        "operation_id":"op-1", "workspace_id":"ws-1", "session_id":"session-1",
        "stage":{"kind":"rewind", "stage":"partial_needs_recovery"}, "error_code":"partial_needs_recovery",
        "published_commit":null, "ref_published_index_pending":false,
        "applied":{"worktree_paths":["src/old.rs", "src/new.rs"], "index_paths":[]},
        "pending_recovery":{"worktree_paths":[], "index_paths":["src/lib.rs"]},
        "unknown":{"worktree_paths":[], "index_paths":["src/lib.rs"]},
        "safety":{"safety_id":"safe-1", "worktree":"durable", "index":"durable", "retention":{"kind":"until_released"}, "verification":"verified"},
        "summary":"已恢复工作树，待核验暂存区"
    });
    result("rewind_executed", rewind.clone());
    assert_wire::<DaemonEvent>(json!({
        "event":"git_operation_progressed", "data":{"operation":rewind, "error":{
            "code":"partial_needs_recovery", "summary_zh":"暂存区待恢复", "impact_scope_zh":"影响选中文件",
            "remediation_zh":"打开恢复预览", "diagnostic_detail":null
        }}
    }));
}

#[test]
fn commit_file_scope_and_ref_published_recovery() {
    let paths = json!(["src/old.rs", "src/new.rs"]);
    command(
        "preview_structured_commit",
        json!({
            "workspace_id":"ws-1", "selected_paths":paths, "message":"提交选中文件"
        }),
    );
    result(
        "structured_commit_previewed",
        json!({
            "operation_id":"op-1", "workspace_id":"ws-1", "parent_oid":PARENT,
            "target_branch_ref":"refs/heads/topic", "selected_paths":paths,
            "files":[{"change":rename(),"source":"worktree","content_digest":"approved","size_bytes":128,"risks":["binary"]}],
            "unselected_staged_paths":[{"kind":"modified","path":"src/other.rs","previous_path":null}],
            "replaced_staged_paths":[rename()],
            "requirements":{"hooks":["pre_commit","prepare_commit_msg","commit_msg"],"hook_adapter_available":true,"signature_required":true},
            "index_sync":{"install_paths":paths,"preserve_staged_paths":["src/other.rs"]},
            "blockers":[], "plan_digest":"digest-1", "expires_at":TIME
        }),
    );
    command(
        "execute_structured_commit",
        json!({
            "operation_id":"op-1", "workspace_id":"ws-1", "selected_paths":paths,
            "plan_digest":"digest-1", "idempotency_key":"idem-1", "expires_at":TIME
        }),
    );
    result("structured_commit_executed", operation());
    for lookup in [
        json!({"kind":"operation_id","operation_id":"op-1"}),
        json!({"kind":"idempotency_key","idempotency_key":"idem-1"}),
    ] {
        command(
            "get_git_operation_status",
            json!({
                "workspace_id":"ws-1", "session_id":null, "kind":"structured_commit", "lookup":lookup
            }),
        );
    }
    result("git_operation_status", operation());
    assert_wire::<DaemonEvent>(
        json!({"event":"git_operation_progressed","data":{"operation":operation(),"error":null}}),
    );
}

#[test]
fn existing_v1_shapes_are_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_wire::<CheckpointAnchor>(json!({
        "checkpoint_id":"cp-old", "turn_index":7, "commit_oid":PARENT,
        "changed_files":1, "diff_stat":"1 file changed"
    }));
    assert_wire::<RpcRequest>(
        json!({"request_id":"old","command":{"method":"get_state_snapshot"}}),
    );
    assert_wire::<RpcResponse>(
        json!({"request_id":"old","ok":true,"result":{"kind":"ack"},"error":null}),
    );
    assert_wire::<EventEnvelope<DaemonEvent>>(json!({
        "protocol_version":1,"seq":7,"timestamp":TIME,"payload":{
            "event":"error_raised","data":{"session_id":null,"error":{
                "code":"conflict","summary_zh":"状态冲突","impact_scope_zh":"当前会话",
                "remediation_zh":"请刷新","diagnostic_detail":null
            }}
        }
    }));
}
