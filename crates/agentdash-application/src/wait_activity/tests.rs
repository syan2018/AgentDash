use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{AgentRunTerminalRegistry, TerminalOutputSnapshot};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeTarget,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxCreateOutcome, AgentRunMailboxMessage, AgentRunMailboxRepository,
    MailboxMessageStatus, NewAgentRunMailboxMessage,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, GateWaitPolicyEnvelope, LifecycleAgent,
    LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository, WaitProducerRef,
};
use agentdash_spi::ExecutionContext;
use agentdash_spi::connector::RuntimeToolProvider;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::types::ResolvedWaitScope;
use super::*;

#[tokio::test]
async fn typed_owner_scope_does_not_require_runtime_binding_inference() {
    let service = test_service(AgentRunTerminalRegistry::new());
    let run_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();

    let scope = service
        .resolve_scope(&WaitToolContext {
            runtime_thread_id: Some(RuntimeThreadId::new("unbound-runtime").unwrap()),
            turn_id: "turn-owner".to_string(),
            owner: Some(WaitActivityOwnerScope {
                run_id,
                agent_id,
                frame_id,
            }),
        })
        .await
        .expect("typed owner scope");

    assert_eq!(scope.run_id, Some(run_id));
    assert_eq!(scope.agent_id, Some(agent_id));
    assert_eq!(scope.frame_id, Some(frame_id));
}

#[tokio::test]
async fn wait_timeout_keeps_running_exec_activity_alive() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.update_state("term-1", "running", None);

    let service = test_service(terminal_registry.clone());
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(result.timed_out);
    assert_eq!(result.items[0].status, "running");
    assert_eq!(
        terminal_registry
            .get_terminal("term-1")
            .expect("terminal")
            .state,
        "running"
    );
}

#[tokio::test]
async fn wait_returns_completed_exec_with_shell_exec_next_ref() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.update_state("term-1", "exited", Some(0));

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].status, "completed");
    assert_eq!(
        result.items[0].exec.as_ref().expect("exec").exit_code,
        Some(0)
    );
    assert_eq!(
        result.items[0].result_refs["source"]["namespace"],
        json!("terminal")
    );
    assert_eq!(result.items[0].result_refs["source"]["kind"], json!("exec"));
    assert_eq!(
        result.items[0].result_refs["output_ref"]["kind"],
        json!("terminal_output")
    );
    assert_eq!(
        result.items[0].result_refs["diagnostic"]["kind"],
        json!("exec_exit")
    );
    assert_eq!(
        result.items[0]
            .next
            .as_ref()
            .and_then(|next| next.get("tool")),
        Some(&json!("shell_exec"))
    );
    assert_eq!(
        result.items[0]
            .next
            .as_ref()
            .and_then(|next| next.get("operation")),
        Some(&json!("read"))
    );
    assert_eq!(
        result.items[0]
            .next
            .as_ref()
            .and_then(|next| next.get("after_seq")),
        Some(&json!(0))
    );
}

#[tokio::test]
async fn wait_returns_failed_exec_for_non_zero_exit_with_diagnostic_refs() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.record_output_snapshot(TerminalOutputSnapshot {
        terminal_id: "term-1",
        stdout: "partial stdout\n",
        stderr: "missing file\n",
        pty: "",
        next_seq: Some(7),
        truncated: false,
        omitted_bytes: 0,
    });
    terminal_registry.update_state("term-1", "exited", Some(2));

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    let item = result.items.first().expect("wait item");
    assert!(!result.timed_out);
    assert_eq!(item.status, "failed");
    assert_eq!(item.exec.as_ref().expect("exec").exit_code, Some(2));
    assert_eq!(
        item.exec
            .as_ref()
            .expect("exec")
            .stderr_preview
            .as_ref()
            .expect("stderr preview")
            .text,
        "missing file"
    );
    assert_eq!(item.result_refs["diagnostic"]["kind"], json!("exec_exit"));
    assert_eq!(item.result_refs["diagnostic"]["exit_code"], json!(2));
    assert_eq!(item.result_refs["output_ref"]["next_seq"], json!(7));
    assert_eq!(item.cursor, Some(item.updated_at_ms.to_string()));
    assert_ne!(item.cursor.as_deref(), Some("7"));
    assert_eq!(
        item.next.as_ref().and_then(|next| next.get("next_seq")),
        Some(&json!(7))
    );
    assert_eq!(
        item.diagnostic.as_ref().expect("diagnostic")["kind"],
        json!("exec_exit")
    );
    assert!(item.preview.as_deref().unwrap_or("").contains("stderr"));
}

#[tokio::test]
async fn wait_maps_cancelled_lost_and_unknown_exec_statuses() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    for (terminal_id, state) in [
        ("term-cancelled", "killed"),
        ("term-lost", "lost"),
        ("term-unknown", "mystery"),
    ] {
        terminal_registry.register_terminal_with_metadata(
            "00000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000002",
            terminal_id,
            "backend-1",
            None,
            Some("main"),
            Some("D:/repo"),
            Some("interactive"),
        );
        terminal_registry.update_state(terminal_id, state, None);
    }

    let service = test_service(terminal_registry);
    let scope = ResolvedWaitScope {
        runtime_thread_id: None,
        run_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("run id")),
        agent_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000002").expect("agent id")),
        frame_id: None,
    };
    let request = WaitActivityRequest {
        activity_refs: Vec::new(),
        kinds: vec!["exec".to_string()],
        timeout_ms: Some(10),
        max_items: Some(10),
        after_cursor: None,
    };

    let items = service
        .collect_items(&scope, &request, &BTreeSet::new())
        .await
        .expect("items");

    assert_eq!(
        items
            .iter()
            .find(|item| item.activity_ref == "term-cancelled")
            .expect("cancelled")
            .status,
        "cancelled"
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.activity_ref == "term-lost")
            .expect("lost")
            .status,
        "lost"
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.activity_ref == "term-unknown")
            .expect("unknown")
            .status,
        "unknown"
    );
}

#[tokio::test]
async fn later_wait_salvages_completed_exec_with_preview_and_read_refs() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.record_output_snapshot(TerminalOutputSnapshot {
        terminal_id: "term-1",
        stdout: "done\n",
        stderr: "",
        pty: "",
        next_seq: Some(3),
        truncated: false,
        omitted_bytes: 0,
    });
    terminal_registry.update_state("term-1", "exited", Some(0));

    let service = test_service(terminal_registry);
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    let item = result.items.first().expect("wait item");
    assert_eq!(item.status, "completed");
    assert_eq!(
        item.exec
            .as_ref()
            .expect("exec")
            .stdout_preview
            .as_ref()
            .expect("stdout preview")
            .text,
        "done"
    );
    assert_eq!(item.result_refs["output_ref"]["after_seq"], json!(0));
    assert_eq!(item.result_refs["output_ref"]["next_seq"], json!(3));
}

#[tokio::test]
async fn wait_timeout_does_not_consume_exec_output_projection() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    terminal_registry.record_output_snapshot(TerminalOutputSnapshot {
        terminal_id: "term-1",
        stdout: "hello\n",
        stderr: "",
        pty: "",
        next_seq: Some(2),
        truncated: false,
        omitted_bytes: 0,
    });
    terminal_registry.update_state("term-1", "running", None);
    let before = terminal_registry
        .get_terminal("term-1")
        .expect("terminal")
        .output_projection
        .expect("projection");

    let service = test_service(terminal_registry.clone());
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    let after = terminal_registry
        .get_terminal("term-1")
        .expect("terminal")
        .output_projection
        .expect("projection");
    assert!(result.timed_out);
    assert_eq!(before.stdout_preview, after.stdout_preview);
    assert_eq!(before.next_seq, after.next_seq);
}

#[tokio::test]
async fn wait_exec_preview_is_bounded_to_terminal_projection_tail() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    let large_stdout = format!("{}tail", "x".repeat(5000));
    terminal_registry.record_output_snapshot(TerminalOutputSnapshot {
        terminal_id: "term-1",
        stdout: &large_stdout,
        stderr: "",
        pty: "",
        next_seq: Some(9),
        truncated: false,
        omitted_bytes: 0,
    });
    terminal_registry.update_state("term-1", "exited", Some(0));

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec!["term-1".to_string()],
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    let preview = result.items[0]
        .exec
        .as_ref()
        .expect("exec")
        .stdout_preview
        .as_ref()
        .expect("stdout preview");
    assert!(preview.truncated);
    assert!(preview.bytes <= 4 * 1024);
    assert!(preview.text.ends_with("tail"));
    assert_eq!(preview.from, "tail");
}

#[tokio::test]
async fn wait_returns_resolved_lifecycle_gate_activity() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let mut gate = LifecycleGate::open(
        Uuid::new_v4(),
        Some(Uuid::new_v4()),
        Some(Uuid::new_v4()),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "done" })),
    );
    let gate_id = gate.id;
    gate.resolve("test");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].kind, "subagent");
    assert_eq!(result.items[0].status, "completed");
    assert_eq!(result.items[0].preview.as_deref(), Some("done"));
}

#[tokio::test]
async fn wait_uses_resolved_lifecycle_gate_payload_status() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let mut gate = LifecycleGate::open(
        Uuid::new_v4(),
        Some(Uuid::new_v4()),
        Some(Uuid::new_v4()),
        "companion_wait_follow_up",
        "dispatch-failed",
        Some(json!({
            "status": "failed",
            "summary": "provider model unsupported",
            "diagnostic": {
                "kind": "provider",
                "code": "invalid_request",
                "http_status": 400,
                "provider": "Example LLM",
                "model": "example-chat-large",
                "message": "request rejected by provider",
                "retryable": false
            }
        })),
    );
    let gate_id = gate.id;
    gate.resolve("runtime_terminal");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(!result.timed_out);
    assert_eq!(result.items[0].kind, "subagent");
    assert_eq!(result.items[0].status, "failed");
    assert_eq!(
        result.items[0].preview.as_deref(),
        Some("provider model unsupported")
    );
    let diagnostic = result.items[0].diagnostic.as_ref().expect("diagnostic");
    assert_eq!(diagnostic["kind"], json!("provider"));
    assert_eq!(diagnostic["code"], json!("invalid_request"));
    assert_eq!(diagnostic["http_status"], json!(400));
    assert_eq!(diagnostic["provider"], json!("Example LLM"));
    assert_eq!(diagnostic["model"], json!("example-chat-large"));
    assert_eq!(diagnostic["retryable"], json!(false));
    assert_eq!(
        result.items[0].result_refs["diagnostic"]["code"],
        json!("invalid_request")
    );
}

#[tokio::test]
async fn wait_exposes_gate_payload_child_evidence_refs() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let run_id = Uuid::new_v4();
    let child_agent_id = Uuid::new_v4();
    let child_frame_id = Uuid::new_v4();
    let mut gate = LifecycleGate::open(
        run_id,
        Some(child_agent_id),
        Some(child_frame_id),
        "companion_wait_follow_up",
        "dispatch-refs",
        Some(json!({
            "status": "failed",
            "summary": "provider model unsupported",
            "result_refs": {
                "schema_version": 1,
                "gate_id": "gate-payload-ref",
                "child": {
                    "run_id": run_id.to_string(),
                    "agent_id": child_agent_id.to_string(),
                    "frame_id": child_frame_id.to_string(),
                    "runtime_thread_id": "child-session"
                },
                "evidence": [
                    {
                        "kind": "lifecycle_file",
                        "scope": "child_delivery_session",
                        "child_run_id": run_id.to_string(),
                        "child_agent_id": child_agent_id.to_string(),
                        "child_frame_id": child_frame_id.to_string(),
                        "runtime_thread_id": "child-session",
                        "mount_id": "lifecycle",
                        "path": "session/events.json"
                    },
                    {
                        "kind": "runtime_trace",
                        "scope": "child_delivery_session",
                        "child_run_id": run_id.to_string(),
                        "child_agent_id": child_agent_id.to_string(),
                        "child_frame_id": child_frame_id.to_string(),
                        "runtime_thread_id": "child-session"
                    }
                ]
            }
        })),
    );
    let gate_id = gate.id;
    gate.resolve("runtime_terminal");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("parent-session").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    let refs = &result.items[0].result_refs;
    assert_eq!(refs["gate_id"], json!(gate_id.to_string()));
    assert_eq!(refs["child"]["run_id"], json!(run_id.to_string()));
    assert_eq!(refs["child"]["runtime_thread_id"], json!("child-session"));
    assert_eq!(refs["evidence"][0]["path"], json!("session/events.json"));
    assert_eq!(refs["evidence"][1]["kind"], json!("runtime_trace"));
    assert!(
        !serde_json::to_string(refs)
            .expect("serialize refs")
            .contains("lifecycle://session/")
    );
}

#[tokio::test]
async fn wait_and_workspace_gate_projection_share_kind_preview_and_status() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let mut gate = LifecycleGate::open(
        Uuid::new_v4(),
        Some(Uuid::new_v4()),
        Some(Uuid::new_v4()),
        "companion_wait_follow_up",
        "dispatch-failed",
        Some(json!({
            "status": "failed",
            "summary": "provider model unsupported",
            "companion_label": "reviewer",
            "source": "producer_terminal"
        })),
    );
    let gate_id = gate.id;
    gate.resolve("runtime_terminal");
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let wait_result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: vec![gate_id.to_string()],
                kinds: vec!["subagent".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");
    let wait_item = wait_result.items.first().expect("wait item");
    let projection = gate.waiting_projection();

    assert_eq!(wait_item.kind, projection.kind);
    assert_eq!(wait_item.preview, projection.preview);
    assert_eq!(wait_item.status, "failed");
}

#[tokio::test]
async fn scoped_gate_wait_keeps_observed_gate_ref_after_resolution() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let run_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();
    let mut gate = LifecycleGate::open(
        run_id,
        Some(agent_id),
        Some(frame_id),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "waiting" })),
    );
    let gate_id = gate.id;
    gate_repo.create(&gate).await.expect("create gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo.clone());
    let scope = ResolvedWaitScope {
        runtime_thread_id: None,
        run_id: Some(run_id),
        agent_id: Some(agent_id),
        frame_id: Some(frame_id),
    };
    let request = WaitActivityRequest {
        activity_refs: Vec::new(),
        kinds: vec!["subagent".to_string()],
        timeout_ms: Some(10),
        max_items: Some(10),
        after_cursor: None,
    };
    let mut observed_refs = BTreeSet::new();

    let pending_items = service
        .collect_items(&scope, &request, &observed_refs)
        .await
        .expect("pending items");
    assert_eq!(pending_items[0].activity_ref, gate_id.to_string());
    assert_eq!(pending_items[0].status, "pending");
    observed_refs.extend(pending_items.into_iter().map(|item| item.activity_ref));

    gate.resolve("test");
    gate_repo.update(&gate).await.expect("resolve gate");

    let completed_items = service
        .collect_items(&scope, &request, &observed_refs)
        .await
        .expect("completed items");
    assert_eq!(completed_items[0].activity_ref, gate_id.to_string());
    assert_eq!(completed_items[0].status, "completed");
}

#[tokio::test]
async fn explicit_gate_ref_is_filtered_by_run_scope_not_current_agent_only() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    let gate_repo = Arc::new(FixtureGateRepo::default());
    let run_id = Uuid::new_v4();
    let parent_agent_id = Uuid::new_v4();
    let child_agent_id = Uuid::new_v4();
    let child_frame_id = Uuid::new_v4();
    let mut child_gate = LifecycleGate::open(
        run_id,
        Some(child_agent_id),
        Some(child_frame_id),
        "companion_wait",
        "dispatch-1",
        Some(json!({ "summary": "done" })),
    );
    let child_gate_id = child_gate.id;
    child_gate.resolve("test");
    gate_repo
        .create(&child_gate)
        .await
        .expect("create child gate");

    let mut other_run_gate = child_gate.clone();
    other_run_gate.id = Uuid::new_v4();
    other_run_gate.run_id = Uuid::new_v4();
    let other_run_gate_id = other_run_gate.id;
    gate_repo
        .create(&other_run_gate)
        .await
        .expect("create other run gate");

    let service = test_service_with_gate_repo(terminal_registry, gate_repo);
    let parent_scope = ResolvedWaitScope {
        runtime_thread_id: None,
        run_id: Some(run_id),
        agent_id: Some(parent_agent_id),
        frame_id: Some(Uuid::new_v4()),
    };
    let request = WaitActivityRequest {
        activity_refs: vec![child_gate_id.to_string(), other_run_gate_id.to_string()],
        kinds: vec!["subagent".to_string()],
        timeout_ms: Some(10),
        max_items: Some(10),
        after_cursor: None,
    };

    let items = service
        .collect_items(&parent_scope, &request, &BTreeSet::new())
        .await
        .expect("items");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].activity_ref, child_gate_id.to_string());
}

#[tokio::test]
async fn wait_after_cursor_filters_older_items() {
    let terminal_registry = AgentRunTerminalRegistry::new();
    terminal_registry.register_terminal_with_metadata(
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
        "term-1",
        "backend-1",
        None,
        Some("main"),
        Some("D:/repo"),
        Some("interactive"),
    );
    let created_at = terminal_registry
        .get_terminal("term-1")
        .expect("terminal")
        .created_at;

    let service = test_service(terminal_registry);
    let result = service
        .wait(
            WaitToolContext {
                runtime_thread_id: Some(RuntimeThreadId::new("runtime-1").unwrap()),
                turn_id: "turn-1".to_string(),
                owner: None,
            },
            WaitActivityRequest {
                activity_refs: Vec::new(),
                kinds: vec!["exec".to_string()],
                timeout_ms: Some(0),
                max_items: Some(10),
                after_cursor: Some(created_at.to_string()),
            },
            CancellationToken::new(),
        )
        .await
        .expect("wait result");

    assert!(result.timed_out);
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn runtime_tool_catalog_includes_wait() {
    let provider =
        WaitRuntimeToolProvider::from_service(test_service(AgentRunTerminalRegistry::new()));
    let composer =
        crate::runtime_tools::provider::SessionRuntimeToolComposer::new(vec![Arc::new(provider)]);
    let context = ExecutionContext {
        session: agentdash_spi::ExecutionSessionFrame {
            turn_id: "runtime-1".to_string(),
            working_directory: std::path::PathBuf::from("."),
            environment_variables: std::collections::HashMap::new(),
            executor_config: agentdash_spi::AgentConfig::new("PI_AGENT"),
            mcp_servers: Vec::new(),
            vfs: None,
            vfs_access_policy: None,
            backend_execution: None,
            runtime_backend_anchor: None,
            identity: None,
        },
        turn: agentdash_spi::ExecutionTurnFrame::default(),
    };

    let tools = composer.build_tools(&context).await.expect("build tools");

    assert!(tools.iter().any(|tool| tool.name() == "wait"));
}

fn test_service(terminal_registry: Arc<AgentRunTerminalRegistry>) -> WaitActivityService {
    test_service_with_gate_repo(terminal_registry, Arc::new(FixtureGateRepo::default()))
}

fn test_service_with_gate_repo(
    terminal_registry: Arc<AgentRunTerminalRegistry>,
    gate_repo: Arc<dyn LifecycleGateRepository>,
) -> WaitActivityService {
    WaitActivityService::from_repositories(
        Arc::new(NoopLifecycleAgentRepo),
        Arc::new(NoopAgentFrameRepo),
        Arc::new(NoopRuntimeBindingRepo),
        gate_repo,
        Arc::new(NoopMailboxRepo),
        terminal_registry,
    )
}

#[derive(Default)]
struct FixtureGateRepo {
    gates: Mutex<HashMap<Uuid, LifecycleGate>>,
}

#[async_trait]
impl LifecycleGateRepository for FixtureGateRepo {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().unwrap().insert(gate.id, gate.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self.gates.lock().unwrap().get(&id).cloned())
    }

    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
            .cloned()
            .collect())
    }

    async fn list_open_gate_wait_policies(
        &self,
        limit: usize,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .filter(|gate| {
                gate.is_open()
                    && gate
                        .payload_json
                        .as_ref()
                        .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                        .is_some()
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn list_by_wait_producer(
        &self,
        producer: &WaitProducerRef,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .filter(|gate| {
                gate.payload_json
                    .as_ref()
                    .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                    .is_some_and(|declaration| declaration.wait_policy.source == *producer)
            })
            .cloned()
            .collect())
    }

    async fn find_by_agent_and_correlation(
        &self,
        agent_id: Uuid,
        correlation_id: &str,
    ) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .unwrap()
            .values()
            .find(|gate| gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id)
            .cloned())
    }

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().unwrap().insert(gate.id, gate.clone());
        Ok(())
    }
}

struct NoopLifecycleAgentRepo;

#[async_trait]
impl LifecycleAgentRepository for NoopLifecycleAgentRepo {
    async fn create(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
        Ok(())
    }

    async fn get(&self, _id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        Ok(None)
    }

    async fn list_by_run(&self, _run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
        Ok(Vec::new())
    }

    async fn update(&self, _agent: &LifecycleAgent) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NoopAgentFrameRepo;

#[async_trait]
impl AgentFrameRepository for NoopAgentFrameRepo {
    async fn create(&self, _frame: &AgentFrame) -> Result<(), DomainError> {
        Ok(())
    }

    async fn get(&self, _frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(None)
    }

    async fn get_current(&self, _agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(None)
    }

    async fn list_by_agent(&self, _agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        Ok(Vec::new())
    }
}

struct NoopRuntimeBindingRepo;

#[async_trait]
impl AgentRunRuntimeBindingRepository for NoopRuntimeBindingRepo {
    async fn load(
        &self,
        _target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(None)
    }

    async fn load_by_thread_id(
        &self,
        _thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(None)
    }

    async fn list_by_run(
        &self,
        _run_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(Vec::new())
    }

    async fn list_by_agent(
        &self,
        _agent_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(Vec::new())
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        Ok(binding)
    }
}

struct NoopMailboxRepo;

#[async_trait]
impl AgentRunMailboxRepository for NoopMailboxRepo {
    async fn create_message(
        &self,
        _message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn create_message_idempotent(
        &self,
        _message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxCreateOutcome, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn get_message(&self, _id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(None)
    }

    async fn list_messages(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(Vec::new())
    }

    async fn claim_next(
        &self,
        _request: agentdash_domain::agent_run_mailbox::AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(Vec::new())
    }

    async fn claim_reconciliation(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _claim_token: Uuid,
        _claim_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(None)
    }

    async fn release_reconciliation_claim(
        &self,
        _id: Uuid,
        _claim_token: Uuid,
        _last_error: String,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn recover_expired_consuming(
        &self,
        _now: chrono::DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }

    async fn mark_message_status(
        &self,
        _id: Uuid,
        _claim_token: Option<Uuid>,
        _status: MailboxMessageStatus,
        _last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn promote_message(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _id: Uuid,
        _priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn delete_message(
        &self,
        _id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(None)
    }

    async fn cleanup_user_payload(&self, _id: Uuid) -> Result<(), DomainError> {
        Ok(())
    }

    async fn pause_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
        _reason: String,
        _message: Option<String>,
    ) -> Result<agentdash_domain::agent_run_mailbox::AgentRunMailboxState, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn resume_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<agentdash_domain::agent_run_mailbox::AgentRunMailboxState, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }

    async fn get_state(
        &self,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<Option<agentdash_domain::agent_run_mailbox::AgentRunMailboxState>, DomainError>
    {
        Ok(None)
    }

    async fn move_message_after(
        &self,
        _id: Uuid,
        _after_id: Option<Uuid>,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        Err(DomainError::InvalidConfig("noop".to_string()))
    }
}
