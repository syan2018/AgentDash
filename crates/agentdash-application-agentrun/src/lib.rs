//! AgentRun current/resource surface, frame construction/update, mailbox, and admission.

pub mod agent_run;
pub mod agent_run_repository_set;
pub mod error;
#[cfg(test)]
pub(crate) mod test_support;

pub use agent_run_repository_set::AgentRunRepositorySet;
pub use error::{ApplicationError, WorkflowApplicationError};
