use std::{
    collections::BTreeMap,
    sync::{PoisonError, RwLock},
};

use agentdash_application_agentrun::agent_run::{AgentRunTerminalRegistry, TerminalOutputSnapshot};
use agentdash_application_vfs::{
    ShellSessionOutputChunk,
    tools::{ShellTerminalOutputSnapshot, ShellTerminalRegistration, ShellTerminalRegistry},
};

#[derive(Debug, Clone)]
pub struct ProcessShellTerminalOutput {
    pub state: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub pty: String,
    pub next_seq: Option<u64>,
    pub truncated: bool,
    pub omitted_bytes: usize,
    pub chunks: Option<Vec<ShellSessionOutputChunk>>,
}

pub struct ProcessShellTerminalRegistry {
    registrations: RwLock<BTreeMap<String, ShellTerminalRegistration>>,
    outputs: RwLock<BTreeMap<String, ProcessShellTerminalOutput>>,
    activity: std::sync::Arc<AgentRunTerminalRegistry>,
}

impl Default for ProcessShellTerminalRegistry {
    fn default() -> Self {
        Self {
            registrations: RwLock::new(BTreeMap::new()),
            outputs: RwLock::new(BTreeMap::new()),
            activity: AgentRunTerminalRegistry::new(),
        }
    }
}

impl ProcessShellTerminalRegistry {
    pub fn activity_registry(&self) -> std::sync::Arc<AgentRunTerminalRegistry> {
        self.activity.clone()
    }

    pub fn output_snapshot(&self, terminal_id: &str) -> Option<ProcessShellTerminalOutput> {
        self.outputs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(terminal_id)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.registrations
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ShellTerminalRegistry for ProcessShellTerminalRegistry {
    fn register_shell_terminal(&self, registration: ShellTerminalRegistration) {
        self.activity.register_runtime_terminal_with_metadata(
            registration.owner.run_id,
            registration.owner.agent_id,
            &registration.owner.runtime_thread_id,
            &registration.terminal_id,
            &registration.backend_id,
            None,
            Some(&registration.mount_id),
            Some(&registration.cwd),
            Some(&registration.capability),
        );
        self.registrations
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(registration.terminal_id.clone(), registration);
    }

    fn resolve_shell_terminal(&self, terminal_id: &str) -> Option<ShellTerminalRegistration> {
        self.registrations
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(terminal_id)
            .cloned()
    }

    fn record_shell_terminal_output_snapshot(&self, snapshot: ShellTerminalOutputSnapshot<'_>) {
        if !self
            .registrations
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(snapshot.terminal_id)
        {
            return;
        }
        self.activity
            .update_state(snapshot.terminal_id, snapshot.state, snapshot.exit_code);
        self.activity
            .record_output_snapshot(TerminalOutputSnapshot {
                terminal_id: snapshot.terminal_id,
                stdout: snapshot.stdout,
                stderr: snapshot.stderr,
                pty: snapshot.pty,
                next_seq: snapshot.next_seq,
                truncated: snapshot.truncated,
                omitted_bytes: snapshot.omitted_bytes,
            });
        self.outputs
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                snapshot.terminal_id.to_owned(),
                ProcessShellTerminalOutput {
                    state: snapshot.state.to_owned(),
                    exit_code: snapshot.exit_code,
                    stdout: snapshot.stdout.to_owned(),
                    stderr: snapshot.stderr.to_owned(),
                    pty: snapshot.pty.to_owned(),
                    next_seq: snapshot.next_seq,
                    truncated: snapshot.truncated,
                    omitted_bytes: snapshot.omitted_bytes,
                    chunks: snapshot.chunks.map(<[_]>::to_vec),
                },
            );
    }

    fn remove_shell_terminal(&self, terminal_id: &str) {
        self.activity.remove_terminal(terminal_id);
        self.registrations
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(terminal_id);
        self.outputs
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(terminal_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_application_vfs::tools::ShellTerminalOwner;
    use uuid::Uuid;

    #[test]
    fn process_registry_resolves_owner_and_retains_latest_output() {
        let registry = ProcessShellTerminalRegistry::default();
        registry.register_shell_terminal(ShellTerminalRegistration {
            owner: ShellTerminalOwner {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
            },
            terminal_id: "terminal-test".to_owned(),
            mount_id: "main".to_owned(),
            backend_id: "backend-test".to_owned(),
            cwd: "main://".to_owned(),
            capability: "shell".to_owned(),
        });
        assert_eq!(
            registry
                .resolve_shell_terminal("terminal-test")
                .expect("registration")
                .backend_id,
            "backend-test"
        );
        registry.record_shell_terminal_output_snapshot(ShellTerminalOutputSnapshot {
            terminal_id: "terminal-test",
            state: "running",
            exit_code: None,
            stdout: "ready",
            stderr: "",
            pty: "",
            next_seq: Some(2),
            truncated: false,
            omitted_bytes: 0,
            chunks: None,
        });
        assert_eq!(
            registry
                .output_snapshot("terminal-test")
                .expect("output")
                .stdout,
            "ready"
        );
        registry.remove_shell_terminal("terminal-test");
        assert!(registry.is_empty());
        assert!(registry.output_snapshot("terminal-test").is_none());
    }
}
