use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::RuntimeThreadStatus;
use agentdash_application_agentrun::agent_run::{
    AgentFrameSurfaceExt, AgentRunRuntime, ConversationModelConfigInput,
    ConversationModelConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigStatusModel,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::lifecycle_read_model::{
    AgentRunView, LifecycleReadModelQueryPort, LifecycleSubjectAssociationView,
};
use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedVfsSurface, ResolvedVfsSurfaceSource, VfsSurfaceRuntimeProjection,
};
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLineage, AgentLineageRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRun,
};
use serde_json::Value;

use crate::ApplicationError;
use crate::agent_run_projection::{
    MAX_AGENT_LINEAGE_DEPTH, lifecycle_agent_title, project_agent_label,
};
use crate::vfs_surface_resolver::VfsSurfaceResolver;

#[derive(Clone)]
pub struct AgentRunProductQuery {
    lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    frame_repo: Arc<dyn AgentFrameRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    project_agent_repo: Arc<dyn ProjectAgentRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
    vfs_surface_resolver: VfsSurfaceResolver,
}

#[derive(Clone)]
pub struct AgentRunProductQueryDeps {
    pub lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub runtime: Arc<dyn AgentRunRuntime>,
    pub vfs_surface_resolver: VfsSurfaceResolver,
}

pub struct AgentRunProductQueryInput<'a> {
    pub run: &'a LifecycleRun,
    pub agent: &'a LifecycleAgent,
    pub has_runtime_binding: bool,
    pub runtime_projection: &'a dyn VfsSurfaceRuntimeProjection,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductModel {
    pub run_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub shell: AgentRunProductShellModel,
    pub agent: AgentRunView,
    pub current_frame: Option<AgentRunCurrentFrameModel>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub lineage: AgentRunProductLineageModel,
    pub resource_surface: Option<ResolvedVfsSurface>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductLineageModel {
    pub parent: Option<AgentRunProductLineageAgentModel>,
    pub children: Vec<AgentRunProductLineageAgentModel>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductLineageAgentModel {
    pub run_id: String,
    pub agent_id: String,
    pub title: String,
    pub lifecycle_status: String,
    pub last_activity_at: String,
    pub runtime: Option<AgentRunProductLineageRuntimeModel>,
    pub children: Vec<AgentRunProductLineageAgentModel>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductLineageRuntimeModel {
    pub thread_status: RuntimeThreadStatus,
    pub active_turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunProductShellModel {
    pub display_title: String,
    pub title_source: String,
    pub lifecycle_status: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunCurrentFrameModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: i32,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub execution_profile: Option<Value>,
    pub model_config: ConversationModelConfigModel,
}

impl AgentRunProductQuery {
    pub fn new(deps: AgentRunProductQueryDeps) -> Self {
        Self {
            lifecycle_read_model_query: deps.lifecycle_read_model_query,
            frame_repo: deps.frame_repo,
            agent_repo: deps.agent_repo,
            lineage_repo: deps.lineage_repo,
            project_agent_repo: deps.project_agent_repo,
            runtime: deps.runtime,
            vfs_surface_resolver: deps.vfs_surface_resolver,
        }
    }

    pub async fn get(
        &self,
        input: AgentRunProductQueryInput<'_>,
    ) -> Result<AgentRunProductModel, ApplicationError> {
        let run_view = self
            .lifecycle_read_model_query
            .lifecycle_run_view(input.run.id)
            .await?;
        let agent_id = input.agent.id.to_string();
        let agent = run_view
            .agents
            .into_iter()
            .find(|agent| agent.agent_ref.agent_id == agent_id)
            .ok_or_else(|| {
                ApplicationError::Conflict(format!(
                    "LifecycleRun {} 缺少 LifecycleAgent {} projection",
                    input.run.id, input.agent.id
                ))
            })?;
        let subject_associations = run_view
            .subject_associations
            .into_iter()
            .filter(|association| {
                association.anchor_agent_id.as_deref() == Some(agent_id.as_str())
                    || association.anchor_agent_id.is_none()
            })
            .collect();
        let current_frame = self
            .frame_repo
            .get_current(input.agent.id)
            .await?
            .map(project_current_frame);
        let resource_surface = if current_frame.is_some() && input.has_runtime_binding {
            Some(
                self.vfs_surface_resolver
                    .resolve_surface(
                        input.runtime_projection,
                        &ResolvedVfsSurfaceSource::AgentRun {
                            run_id: input.run.id,
                            agent_id: input.agent.id,
                        },
                    )
                    .await?,
            )
        } else {
            None
        };
        let lineage = self
            .load_lineage(input.run.id, input.run.project_id, input.agent.id)
            .await?;

        Ok(AgentRunProductModel {
            run_id: input.run.id.to_string(),
            agent_id,
            project_id: input.run.project_id.to_string(),
            shell: AgentRunProductShellModel {
                display_title: input
                    .agent
                    .workspace_title
                    .clone()
                    .unwrap_or_else(|| input.agent.source.as_str().to_string()),
                title_source: input
                    .agent
                    .workspace_title_source
                    .clone()
                    .unwrap_or_else(|| "agent_source".to_string()),
                lifecycle_status: input.agent.status.clone(),
                last_activity_at: input.run.last_activity_at.to_rfc3339(),
            },
            agent,
            current_frame,
            subject_associations,
            lineage,
            resource_surface,
        })
    }

    async fn load_lineage(
        &self,
        run_id: uuid::Uuid,
        project_id: uuid::Uuid,
        current_agent_id: uuid::Uuid,
    ) -> Result<AgentRunProductLineageModel, ApplicationError> {
        let agents = self.agent_repo.list_by_run(run_id).await?;
        let lineages = self.lineage_repo.list_by_run(run_id).await?;
        let graph = ProductLineageGraph::new(run_id, &agents, &lineages);
        let related_agent_ids = graph.related_agent_ids(current_agent_id);
        let related_agent_id_set = related_agent_ids.iter().copied().collect::<HashSet<_>>();
        let mut seen_project_agent_ids = HashSet::new();
        let mut project_agent_labels = HashMap::new();
        for project_agent_id in agents
            .iter()
            .filter(|agent| related_agent_id_set.contains(&agent.id))
            .filter_map(|agent| agent.project_agent_id)
            .filter(|project_agent_id| seen_project_agent_ids.insert(*project_agent_id))
        {
            if let Some(project_agent) = self
                .project_agent_repo
                .get_by_project_and_id(project_id, project_agent_id)
                .await?
            {
                project_agent_labels.insert(project_agent.id, project_agent_label(&project_agent));
            }
        }
        let mut runtime_by_agent_id = HashMap::new();
        for agent_id in related_agent_ids {
            let runtime = self
                .runtime
                .inspect(AgentRunRuntimeTarget { run_id, agent_id })
                .await
                .map_err(|error| {
                    ApplicationError::Internal(format!(
                        "AgentRun detail lineage runtime inspect failed: run_id={run_id}, agent_id={agent_id}: {error}"
                    ))
                })?
                .snapshot
                .map(|snapshot| AgentRunProductLineageRuntimeModel {
                    thread_status: snapshot.status,
                    active_turn_id: snapshot.active_turn_id.map(|id| id.to_string()),
                });
            runtime_by_agent_id.insert(agent_id, runtime);
        }
        Ok(graph.project(
            current_agent_id,
            &runtime_by_agent_id,
            &project_agent_labels,
        ))
    }
}

struct ProductLineageGraph<'a> {
    run_id: uuid::Uuid,
    agents: HashMap<uuid::Uuid, &'a LifecycleAgent>,
    parent_by_child: HashMap<uuid::Uuid, uuid::Uuid>,
    children_by_parent: HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
}

impl<'a> ProductLineageGraph<'a> {
    fn new(run_id: uuid::Uuid, agents: &'a [LifecycleAgent], lineages: &[AgentLineage]) -> Self {
        let agents = agents
            .iter()
            .filter(|agent| agent.run_id == run_id)
            .map(|agent| (agent.id, agent))
            .collect::<HashMap<_, _>>();
        let mut parent_by_child = HashMap::new();
        let mut children_by_parent = HashMap::<uuid::Uuid, Vec<uuid::Uuid>>::new();
        for lineage in lineages {
            let Some(parent_id) = lineage.parent_agent_id else {
                continue;
            };
            if lineage.run_id != run_id
                || parent_id == lineage.child_agent_id
                || !agents.contains_key(&parent_id)
                || !agents.contains_key(&lineage.child_agent_id)
                || parent_by_child.contains_key(&lineage.child_agent_id)
            {
                continue;
            }
            parent_by_child.insert(lineage.child_agent_id, parent_id);
            children_by_parent
                .entry(parent_id)
                .or_default()
                .push(lineage.child_agent_id);
        }
        Self {
            run_id,
            agents,
            parent_by_child,
            children_by_parent,
        }
    }

    fn related_agent_ids(&self, current_agent_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        let mut ids = Vec::new();
        let mut visited = HashSet::from([current_agent_id]);
        if let Some(parent_id) = self.parent_id(current_agent_id)
            && visited.insert(parent_id)
        {
            ids.push(parent_id);
        }
        self.collect_descendant_ids(current_agent_id, 0, &mut visited, &mut ids);
        ids
    }

    fn project(
        &self,
        current_agent_id: uuid::Uuid,
        runtime_by_agent_id: &HashMap<uuid::Uuid, Option<AgentRunProductLineageRuntimeModel>>,
        project_agent_labels: &HashMap<uuid::Uuid, String>,
    ) -> AgentRunProductLineageModel {
        let mut visited = HashSet::from([current_agent_id]);
        let parent = self.parent_id(current_agent_id).and_then(|parent_id| {
            visited.insert(parent_id);
            self.project_agent(
                parent_id,
                Vec::new(),
                runtime_by_agent_id,
                project_agent_labels,
            )
        });
        let children = self.project_children(
            current_agent_id,
            0,
            &mut visited,
            runtime_by_agent_id,
            project_agent_labels,
        );
        AgentRunProductLineageModel { parent, children }
    }

    fn parent_id(&self, current_agent_id: uuid::Uuid) -> Option<uuid::Uuid> {
        let parent_id = *self.parent_by_child.get(&current_agent_id)?;
        (!self.is_reachable(current_agent_id, parent_id)).then_some(parent_id)
    }

    fn is_reachable(&self, start: uuid::Uuid, target: uuid::Uuid) -> bool {
        let mut stack = vec![start];
        let mut visited = HashSet::new();
        while let Some(agent_id) = stack.pop() {
            if !visited.insert(agent_id) {
                continue;
            }
            for child_id in self.children_by_parent.get(&agent_id).into_iter().flatten() {
                if *child_id == target {
                    return true;
                }
                stack.push(*child_id);
            }
        }
        false
    }

    fn collect_descendant_ids(
        &self,
        parent_id: uuid::Uuid,
        depth: usize,
        visited: &mut HashSet<uuid::Uuid>,
        ids: &mut Vec<uuid::Uuid>,
    ) {
        if depth >= MAX_AGENT_LINEAGE_DEPTH {
            return;
        }
        for child_id in self
            .children_by_parent
            .get(&parent_id)
            .into_iter()
            .flatten()
        {
            if !visited.insert(*child_id) {
                continue;
            }
            ids.push(*child_id);
            self.collect_descendant_ids(*child_id, depth + 1, visited, ids);
        }
    }

    fn project_children(
        &self,
        parent_id: uuid::Uuid,
        depth: usize,
        visited: &mut HashSet<uuid::Uuid>,
        runtime_by_agent_id: &HashMap<uuid::Uuid, Option<AgentRunProductLineageRuntimeModel>>,
        project_agent_labels: &HashMap<uuid::Uuid, String>,
    ) -> Vec<AgentRunProductLineageAgentModel> {
        if depth >= MAX_AGENT_LINEAGE_DEPTH {
            return Vec::new();
        }
        self.children_by_parent
            .get(&parent_id)
            .into_iter()
            .flatten()
            .filter_map(|child_id| {
                if !visited.insert(*child_id) {
                    return None;
                }
                let children = self.project_children(
                    *child_id,
                    depth + 1,
                    visited,
                    runtime_by_agent_id,
                    project_agent_labels,
                );
                self.project_agent(
                    *child_id,
                    children,
                    runtime_by_agent_id,
                    project_agent_labels,
                )
            })
            .collect()
    }

    fn project_agent(
        &self,
        agent_id: uuid::Uuid,
        children: Vec<AgentRunProductLineageAgentModel>,
        runtime_by_agent_id: &HashMap<uuid::Uuid, Option<AgentRunProductLineageRuntimeModel>>,
        project_agent_labels: &HashMap<uuid::Uuid, String>,
    ) -> Option<AgentRunProductLineageAgentModel> {
        let agent = self.agents.get(&agent_id)?;
        Some(AgentRunProductLineageAgentModel {
            run_id: self.run_id.to_string(),
            agent_id: agent.id.to_string(),
            title: lifecycle_agent_title(agent, project_agent_labels),
            lifecycle_status: agent.status.clone(),
            last_activity_at: agent.updated_at.to_rfc3339(),
            runtime: runtime_by_agent_id.get(&agent_id).cloned().flatten(),
            children,
        })
    }
}

fn project_current_frame(frame: AgentFrame) -> AgentRunCurrentFrameModel {
    let execution_profile = frame.typed_execution_profile();
    let model_config = if let Some(execution_profile) = execution_profile.as_ref() {
        ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            frame_execution_profile: Some(execution_profile),
            ..Default::default()
        })
        .view
    } else {
        ConversationModelConfigModel {
            status: ConversationModelConfigStatusModel::ModelRequired,
            effective_executor_config: None,
            missing_fields: vec!["execution_profile".to_string()],
            message: Some("current AgentFrame 缺少可解析的 execution profile。".to_string()),
        }
    };

    AgentRunCurrentFrameModel {
        agent_id: frame.agent_id.to_string(),
        frame_id: frame.id.to_string(),
        revision: frame.revision,
        capability_surface: frame
            .effective_capability_json
            .unwrap_or(serde_json::Value::Null),
        context_slice: frame.context_slice_json.unwrap_or(serde_json::Value::Null),
        vfs_surface: frame.vfs_surface_json.unwrap_or(serde_json::Value::Null),
        mcp_surface: frame.mcp_surface_json.unwrap_or(serde_json::Value::Null),
        execution_profile: frame.execution_profile_json,
        model_config,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use agentdash_agent_runtime_contract::RuntimeThreadStatus;
    use agentdash_application_agentrun::agent_run::{
        ConversationModelConfigSourceModel, ConversationModelConfigStatusModel,
    };
    use agentdash_domain::workflow::{AgentFrame, AgentLineage, AgentSource, LifecycleAgent};
    use serde_json::json;
    use uuid::Uuid;

    use super::{ProductLineageGraph, project_current_frame};

    fn lineage_agent(
        run_id: Uuid,
        project_id: Uuid,
        source: AgentSource,
        title: &str,
    ) -> LifecycleAgent {
        let mut agent = LifecycleAgent::new_root(run_id, project_id, source);
        agent.workspace_title = Some(title.to_string());
        agent
    }

    fn projected_agent_ids(
        nodes: &[super::AgentRunProductLineageAgentModel],
        ids: &mut Vec<String>,
    ) {
        for node in nodes {
            ids.push(node.agent_id.clone());
            projected_agent_ids(&node.children, ids);
        }
    }

    #[test]
    fn detail_lineage_projects_canonical_parent_and_recursive_children() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let parent = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Parent");
        let current = lineage_agent(run_id, project_id, AgentSource::Subagent, "Current");
        let child = lineage_agent(run_id, project_id, AgentSource::Subagent, "Child");
        let grandchild = lineage_agent(run_id, project_id, AgentSource::Subagent, "Grandchild");
        let agents = vec![
            parent.clone(),
            current.clone(),
            child.clone(),
            grandchild.clone(),
        ];
        let lineages = vec![
            AgentLineage::new(run_id, Some(parent.id), current.id, "subagent", None, None),
            AgentLineage::new(run_id, Some(current.id), child.id, "subagent", None, None),
            AgentLineage::new(
                run_id,
                Some(child.id),
                grandchild.id,
                "subagent",
                None,
                None,
            ),
        ];

        let runtime_by_agent_id = HashMap::from([(
            child.id,
            Some(super::AgentRunProductLineageRuntimeModel {
                thread_status: RuntimeThreadStatus::Active,
                active_turn_id: Some("turn-1".to_string()),
            }),
        )]);
        let model = ProductLineageGraph::new(run_id, &agents, &lineages).project(
            current.id,
            &runtime_by_agent_id,
            &HashMap::new(),
        );

        assert_eq!(
            model.parent.as_ref().map(|agent| agent.agent_id.clone()),
            Some(parent.id.to_string())
        );
        assert_eq!(model.children.len(), 1);
        assert_eq!(model.children[0].agent_id, child.id.to_string());
        assert_eq!(
            model.children[0]
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.active_turn_id.as_deref()),
            Some("turn-1")
        );
        assert_eq!(model.children[0].children.len(), 1);
        assert_eq!(
            model.children[0].children[0].agent_id,
            grandchild.id.to_string()
        );
    }

    #[test]
    fn detail_lineage_stops_cycles_without_repeating_current_agent() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let current = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Current");
        let child = lineage_agent(run_id, project_id, AgentSource::Subagent, "Child");
        let grandchild = lineage_agent(run_id, project_id, AgentSource::Subagent, "Grandchild");
        let agents = vec![current.clone(), child.clone(), grandchild.clone()];
        let lineages = vec![
            AgentLineage::new(run_id, Some(current.id), child.id, "subagent", None, None),
            AgentLineage::new(
                run_id,
                Some(child.id),
                grandchild.id,
                "subagent",
                None,
                None,
            ),
            AgentLineage::new(
                run_id,
                Some(grandchild.id),
                current.id,
                "invalid_cycle",
                None,
                None,
            ),
        ];

        let model = ProductLineageGraph::new(run_id, &agents, &lineages).project(
            current.id,
            &HashMap::new(),
            &HashMap::new(),
        );
        let mut ids = Vec::new();
        projected_agent_ids(&model.children, &mut ids);

        assert!(model.parent.is_none());
        assert_eq!(ids, vec![child.id.to_string(), grandchild.id.to_string()]);
        assert!(!ids.contains(&current.id.to_string()));
    }

    #[test]
    fn detail_lineage_ignores_orphan_and_cross_run_edges() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let current = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Current");
        let child = lineage_agent(run_id, project_id, AgentSource::Subagent, "Child");
        let orphan = lineage_agent(run_id, project_id, AgentSource::Subagent, "Orphan");
        let agents = vec![current.clone(), child.clone(), orphan.clone()];
        let lineages = vec![
            AgentLineage::new(run_id, Some(current.id), child.id, "subagent", None, None),
            AgentLineage::new(
                run_id,
                Some(Uuid::new_v4()),
                orphan.id,
                "orphan",
                None,
                None,
            ),
            AgentLineage::new(
                Uuid::new_v4(),
                Some(current.id),
                orphan.id,
                "cross_run",
                None,
                None,
            ),
        ];

        let model = ProductLineageGraph::new(run_id, &agents, &lineages).project(
            current.id,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(model.children.len(), 1);
        assert_eq!(model.children[0].agent_id, child.id.to_string());
    }

    #[test]
    fn detail_lineage_projects_each_related_agent_once() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let current = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Current");
        let child = lineage_agent(run_id, project_id, AgentSource::Subagent, "Child");
        let grandchild = lineage_agent(run_id, project_id, AgentSource::Subagent, "Grandchild");
        let agents = vec![current.clone(), child.clone(), grandchild.clone()];
        let lineages = vec![
            AgentLineage::new(run_id, Some(current.id), child.id, "first", None, None),
            AgentLineage::new(run_id, Some(current.id), child.id, "duplicate", None, None),
            AgentLineage::new(
                run_id,
                Some(child.id),
                grandchild.id,
                "first_parent",
                None,
                None,
            ),
            AgentLineage::new(
                run_id,
                Some(current.id),
                grandchild.id,
                "second_parent",
                None,
                None,
            ),
        ];

        let model = ProductLineageGraph::new(run_id, &agents, &lineages).project(
            current.id,
            &HashMap::new(),
            &HashMap::new(),
        );
        let mut ids = Vec::new();
        projected_agent_ids(&model.children, &mut ids);
        let unique = ids.iter().collect::<HashSet<_>>();

        assert_eq!(ids.len(), 2);
        assert_eq!(unique.len(), 2);
        assert_eq!(ids, vec![child.id.to_string(), grandchild.id.to_string()]);
    }

    #[test]
    fn detail_lineage_uses_project_agent_label_without_workspace_title() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let current = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Current");
        let mut child = LifecycleAgent::new_root(run_id, project_id, AgentSource::Subagent);
        let project_agent_id = Uuid::new_v4();
        child.project_agent_id = Some(project_agent_id);
        let agents = vec![current.clone(), child.clone()];
        let lineages = vec![AgentLineage::new(
            run_id,
            Some(current.id),
            child.id,
            "subagent",
            None,
            None,
        )];
        let project_agent_labels = HashMap::from([(project_agent_id, "Code Reviewer".to_string())]);

        let model = ProductLineageGraph::new(run_id, &agents, &lineages).project(
            current.id,
            &HashMap::new(),
            &project_agent_labels,
        );

        assert_eq!(model.children[0].title, "Code Reviewer");
    }

    #[test]
    fn detail_lineage_bounds_projection_and_runtime_inspection_candidates() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let root = lineage_agent(run_id, project_id, AgentSource::ProjectAgent, "Root");
        let mut agents = vec![root.clone()];
        let mut lineages = Vec::new();
        let mut parent_id = root.id;
        for index in 0..(crate::agent_run_projection::MAX_AGENT_LINEAGE_DEPTH + 3) {
            let child = lineage_agent(
                run_id,
                project_id,
                AgentSource::Subagent,
                &format!("Child {index}"),
            );
            lineages.push(AgentLineage::new(
                run_id,
                Some(parent_id),
                child.id,
                "subagent",
                None,
                None,
            ));
            parent_id = child.id;
            agents.push(child);
        }
        let labels = HashMap::new();
        let graph = ProductLineageGraph::new(run_id, &agents, &lineages);

        let runtime_inspection_candidates = graph.related_agent_ids(root.id);
        let model = graph.project(root.id, &HashMap::new(), &labels);
        let mut projected_ids = Vec::new();
        projected_agent_ids(&model.children, &mut projected_ids);

        assert_eq!(
            runtime_inspection_candidates.len(),
            crate::agent_run_projection::MAX_AGENT_LINEAGE_DEPTH
        );
        assert_eq!(
            projected_ids.len(),
            crate::agent_run_projection::MAX_AGENT_LINEAGE_DEPTH
        );
    }

    #[test]
    fn current_frame_projects_effective_model_config_from_execution_profile() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.execution_profile_json = Some(json!({
            "executor": "PI_AGENT",
            "provider_id": "openai",
            "model_id": "gpt-test"
        }));
        frame.effective_capability_json = Some(json!({ "version": 1 }));

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::Resolved
        );
        let effective = model
            .model_config
            .effective_executor_config
            .expect("effective config");
        assert_eq!(effective.executor, "PI_AGENT");
        assert_eq!(effective.provider_id.as_deref(), Some("openai"));
        assert_eq!(effective.model_id.as_deref(), Some("gpt-test"));
        assert_eq!(
            effective.source,
            ConversationModelConfigSourceModel::FrameExecutionProfile
        );
        assert_eq!(model.capability_surface, json!({ "version": 1 }));
    }

    #[test]
    fn current_frame_reports_missing_cloud_native_model_fields() {
        let mut frame = AgentFrame::new_initial(Uuid::new_v4());
        frame.execution_profile_json = Some(json!({ "executor": "PI_AGENT" }));

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::ModelRequired
        );
        assert_eq!(
            model.model_config.missing_fields,
            vec!["provider_id".to_string(), "model_id".to_string()]
        );
    }

    #[test]
    fn current_frame_does_not_invent_default_executor_without_execution_profile() {
        let frame = AgentFrame::new_initial(Uuid::new_v4());

        let model = project_current_frame(frame);

        assert_eq!(
            model.model_config.status,
            ConversationModelConfigStatusModel::ModelRequired
        );
        assert!(model.model_config.effective_executor_config.is_none());
        assert_eq!(
            model.model_config.missing_fields,
            vec!["execution_profile".to_string()]
        );
    }
}
