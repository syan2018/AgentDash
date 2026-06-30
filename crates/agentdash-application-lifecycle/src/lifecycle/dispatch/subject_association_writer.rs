use uuid::Uuid;

use agentdash_domain::workflow::{
    ExecutionSource, LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    SubjectExecutionRef,
};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::DispatchPlan;

pub(crate) struct SubjectAssociationWriter<'a> {
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
}

pub(crate) struct SubjectAssociationWriteResult {
    pub(crate) subject_execution_ref: Option<SubjectExecutionRef>,
}

impl<'a> SubjectAssociationWriter<'a> {
    pub(crate) fn new(association_repo: &'a dyn LifecycleSubjectAssociationRepository) -> Self {
        Self { association_repo }
    }

    pub(crate) async fn write_for_dispatch(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        plan: &DispatchPlan,
    ) -> Result<SubjectAssociationWriteResult, WorkflowApplicationError> {
        let Some(subject_ref) = &plan.subject_ref else {
            return Ok(SubjectAssociationWriteResult {
                subject_execution_ref: None,
            });
        };

        let role = association_role_from_source(&plan.source);
        let association = if matches!(subject_ref.kind.as_str(), "task" | "story") {
            LifecycleSubjectAssociation::new_agent_scoped(run_id, agent_id, subject_ref, role, None)
        } else {
            LifecycleSubjectAssociation::new_run_scoped(run_id, subject_ref, role, None)
        };
        self.association_repo.create(&association).await?;

        Ok(SubjectAssociationWriteResult {
            subject_execution_ref: Some(SubjectExecutionRef {
                subject_ref: subject_ref.clone(),
                association_id: association.id,
            }),
        })
    }
}

fn association_role_from_source(source: &ExecutionSource) -> &'static str {
    match source {
        ExecutionSource::Routine => "source",
        _ => "subject",
    }
}
