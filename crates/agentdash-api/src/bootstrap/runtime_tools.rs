use std::sync::Arc;

use agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry;
use agentdash_application_agentrun::agent_run::terminal_registry::{
    TerminalOutputChunkSnapshot, TerminalOutputDeltaSnapshot, TerminalOutputSnapshot,
};
use agentdash_application_vfs::tools::{
    ShellTerminalOutputSnapshot, ShellTerminalOwner, ShellTerminalRegistration,
    ShellTerminalRegistry,
};

#[derive(Clone)]
struct RuntimeShellTerminalRegistry {
    terminal_registry: Arc<AgentRunTerminalRegistry>,
}

impl RuntimeShellTerminalRegistry {
    fn new(terminal_registry: Arc<AgentRunTerminalRegistry>) -> Self {
        Self { terminal_registry }
    }
}

impl ShellTerminalRegistry for RuntimeShellTerminalRegistry {
    fn register_shell_terminal(&self, registration: ShellTerminalRegistration) {
        let ShellTerminalRegistration {
            owner,
            terminal_id,
            mount_id,
            backend_id,
            cwd,
            capability,
        } = registration;
        self.terminal_registry.bind_session(
            owner.runtime_thread_id.as_str(),
            &owner.run_id.to_string(),
            &owner.agent_id.to_string(),
        );
        self.terminal_registry
            .register_runtime_terminal_with_metadata(
                owner.run_id,
                owner.agent_id,
                &owner.runtime_thread_id,
                &terminal_id,
                &backend_id,
                None,
                Some(&mount_id),
                Some(&cwd),
                Some(&capability),
            );
    }

    fn resolve_shell_terminal(&self, terminal_id: &str) -> Option<ShellTerminalRegistration> {
        let state = self.terminal_registry.get_terminal(terminal_id)?;
        Some(ShellTerminalRegistration {
            owner: ShellTerminalOwner {
                run_id: uuid::Uuid::parse_str(&state.run_id).ok()?,
                agent_id: uuid::Uuid::parse_str(&state.agent_id).ok()?,
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    state.runtime_thread_id?,
                )
                .ok()?,
            },
            terminal_id: state.terminal_id,
            mount_id: state.mount_id?,
            backend_id: state.backend_id,
            cwd: state.cwd.unwrap_or_default(),
            capability: state.capability.unwrap_or_else(|| "state_only".to_string()),
        })
    }

    fn record_shell_terminal_output_snapshot(&self, snapshot: ShellTerminalOutputSnapshot<'_>) {
        if let (Some(chunks), Some(next_seq)) = (snapshot.chunks, snapshot.next_seq) {
            let chunks = chunks
                .iter()
                .map(|chunk| TerminalOutputChunkSnapshot {
                    seq: chunk.seq,
                    stream: &chunk.stream,
                    data: &chunk.data,
                })
                .collect::<Vec<_>>();
            self.terminal_registry
                .record_output_delta(TerminalOutputDeltaSnapshot {
                    terminal_id: snapshot.terminal_id,
                    chunks: &chunks,
                    next_seq,
                    truncated: snapshot.truncated,
                    omitted_bytes: snapshot.omitted_bytes,
                });
        } else {
            self.terminal_registry
                .record_output_snapshot(TerminalOutputSnapshot {
                    terminal_id: snapshot.terminal_id,
                    stdout: snapshot.stdout,
                    stderr: snapshot.stderr,
                    pty: snapshot.pty,
                    next_seq: snapshot.next_seq,
                    truncated: snapshot.truncated,
                    omitted_bytes: snapshot.omitted_bytes,
                });
        }
        self.terminal_registry.update_state(
            snapshot.terminal_id,
            terminal_projection_state(snapshot.state),
            snapshot.exit_code,
        );
    }

    fn remove_shell_terminal(&self, terminal_id: &str) {
        self.terminal_registry.remove_terminal(terminal_id);
    }
}

fn terminal_projection_state(state: &str) -> &str {
    match state {
        "completed" | "failed" | "timed_out" | "closed" => "exited",
        state => state,
    }
}

pub(crate) fn build_shell_terminal_registry_adapter(
    terminal_registry: Arc<AgentRunTerminalRegistry>,
) -> Arc<dyn ShellTerminalRegistry> {
    Arc::new(RuntimeShellTerminalRegistry::new(terminal_registry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> ShellTerminalOwner {
        ShellTerminalOwner {
            run_id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
            agent_id: uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                .expect("agent id"),
            runtime_thread_id: "runtime-thread-shell".parse().expect("runtime thread"),
        }
    }

    #[test]
    fn adapter_registers_typed_owner_and_binds_canonical_runtime_thread() {
        let registry = AgentRunTerminalRegistry::new();
        let adapter = build_shell_terminal_registry_adapter(registry.clone());
        let owner = owner();

        adapter.register_shell_terminal(ShellTerminalRegistration {
            owner: owner.clone(),
            terminal_id: "term-shell".to_string(),
            mount_id: "main".to_string(),
            backend_id: "backend-local".to_string(),
            cwd: "main://".to_string(),
            capability: "read_only_output".to_string(),
        });

        let terminal = registry.get_terminal("term-shell").expect("terminal");
        assert_eq!(
            terminal.runtime_thread_id.as_deref(),
            Some(owner.runtime_thread_id.as_str())
        );
        assert_eq!(
            registry
                .resolve_active_session(&owner.run_id.to_string(), &owner.agent_id.to_string())
                .as_deref(),
            Some(owner.runtime_thread_id.as_str())
        );
        assert_eq!(
            adapter
                .resolve_shell_terminal("term-shell")
                .expect("registration")
                .owner,
            owner
        );
    }

    #[test]
    fn adapter_projects_retained_output_and_terminal_state() {
        let registry = AgentRunTerminalRegistry::new();
        let adapter = build_shell_terminal_registry_adapter(registry.clone());
        adapter.register_shell_terminal(ShellTerminalRegistration {
            owner: owner(),
            terminal_id: "term-shell".to_string(),
            mount_id: "main".to_string(),
            backend_id: "backend-local".to_string(),
            cwd: "main://".to_string(),
            capability: "read_only_output".to_string(),
        });

        adapter.record_shell_terminal_output_snapshot(ShellTerminalOutputSnapshot {
            terminal_id: "term-shell",
            state: "completed",
            exit_code: Some(0),
            stdout: "retained output\n",
            stderr: "",
            pty: "",
            next_seq: Some(3),
            truncated: false,
            omitted_bytes: 0,
            chunks: None,
        });

        let incremental_chunks = vec![agentdash_application_vfs::ShellSessionOutputChunk {
            seq: 3,
            stream: "stdout".to_string(),
            data: "continued output\n".to_string(),
        }];
        adapter.record_shell_terminal_output_snapshot(ShellTerminalOutputSnapshot {
            terminal_id: "term-shell",
            state: "completed",
            exit_code: Some(0),
            stdout: "continued output\n",
            stderr: "",
            pty: "",
            next_seq: Some(4),
            truncated: false,
            omitted_bytes: 0,
            chunks: Some(&incremental_chunks),
        });

        let terminal = registry.get_terminal("term-shell").expect("terminal");
        assert_eq!(terminal.state, "exited");
        assert_eq!(terminal.exit_code, Some(0));
        let output = terminal.output_projection.expect("output projection");
        assert_eq!(output.next_seq, Some(4));
        assert_eq!(
            output.stdout_preview.expect("stdout preview").text,
            "retained output\ncontinued output"
        );
    }
}
