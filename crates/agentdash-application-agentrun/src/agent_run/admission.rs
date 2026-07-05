use crate::agent_run::{
    AgentRunForkCommand, AgentRunForkCommandResult, AgentRunForkService, AgentRunForkSubmitCommand,
    ProjectAgentRunStartCommand, ProjectAgentRunStartDispatch, ProjectAgentRunStartService,
};
use crate::error::WorkflowApplicationError;

pub struct AgentRunAdmissionService<'a> {
    boundary: AgentRunAdmissionBoundary<'a>,
}

enum AgentRunAdmissionBoundary<'a> {
    ProjectAgentStart(ProjectAgentRunStartService<'a>),
    Fork(AgentRunForkService<'a>),
}

impl<'a> AgentRunAdmissionService<'a> {
    pub fn for_project_agent_start(start: ProjectAgentRunStartService<'a>) -> Self {
        Self {
            boundary: AgentRunAdmissionBoundary::ProjectAgentStart(start),
        }
    }

    pub fn for_fork(fork: AgentRunForkService<'a>) -> Self {
        Self {
            boundary: AgentRunAdmissionBoundary::Fork(fork),
        }
    }

    pub async fn admit_project_agent_start(
        &self,
        command: ProjectAgentRunStartCommand,
    ) -> Result<ProjectAgentRunStartDispatch, WorkflowApplicationError> {
        match &self.boundary {
            AgentRunAdmissionBoundary::ProjectAgentStart(start) => start.start_run(command).await,
            AgentRunAdmissionBoundary::Fork(_) => Err(wrong_boundary("project-agent start")),
        }
    }

    pub async fn admit_explicit_fork(
        &self,
        command: AgentRunForkCommand,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        match &self.boundary {
            AgentRunAdmissionBoundary::Fork(fork) => fork.explicit_fork(command).await,
            AgentRunAdmissionBoundary::ProjectAgentStart(_) => Err(wrong_boundary("fork")),
        }
    }

    pub async fn admit_fork_submit(
        &self,
        command: AgentRunForkSubmitCommand,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        match &self.boundary {
            AgentRunAdmissionBoundary::Fork(fork) => fork.fork_submit(command).await,
            AgentRunAdmissionBoundary::ProjectAgentStart(_) => Err(wrong_boundary("fork-submit")),
        }
    }
}

fn wrong_boundary(command: &str) -> WorkflowApplicationError {
    WorkflowApplicationError::Internal(format!(
        "AgentRun admission boundary does not support {command}"
    ))
}
