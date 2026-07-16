use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use agentdash_agent_runtime_contract::RuntimeThreadStatus;
use agentdash_application_agentrun::agent_run::AgentRunRuntime;
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_domain::{
    agent::ProjectAgentRepository,
    workflow::{
        AgentLineage, AgentLineageRepository, LifecycleAgent, LifecycleAgentRepository,
        LifecycleRun, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    },
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;

use crate::ApplicationError;
use crate::agent_run_projection::{
    MAX_AGENT_LINEAGE_DEPTH, lifecycle_agent_title, project_agent_label,
};

const DEFAULT_PAGE_LIMIT: usize = 30;
const MAX_PAGE_LIMIT: usize = 100;

#[derive(Clone)]
pub struct ProjectAgentRunListQuery {
    run_repo: Arc<dyn LifecycleRunRepository>,
    agent_repo: Arc<dyn LifecycleAgentRepository>,
    lineage_repo: Arc<dyn AgentLineageRepository>,
    subject_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    project_agent_repo: Arc<dyn ProjectAgentRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
}

#[derive(Clone)]
pub struct ProjectAgentRunListQueryDeps {
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
    pub subject_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub project_agent_repo: Arc<dyn ProjectAgentRepository>,
    pub runtime: Arc<dyn AgentRunRuntime>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunListInput<'a> {
    pub project_id: Uuid,
    pub limit: Option<usize>,
    pub cursor: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunListPage {
    pub project_id: Uuid,
    pub entries: Vec<AgentRunListEntryModel>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunListEntryModel {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub title: String,
    pub source: String,
    pub lifecycle_status: String,
    pub last_activity_at: String,
    pub project_agent_label: Option<String>,
    pub runtime: Option<AgentRunListRuntimeSummaryModel>,
    pub subject: Option<AgentRunListSubjectModel>,
    pub subagent_count: u32,
    pub children: Vec<AgentRunListChildModel>,
}

#[derive(Debug, Clone)]
pub struct AgentRunListChildModel {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub title: String,
    pub source: String,
    pub lifecycle_status: String,
    pub last_activity_at: String,
    pub project_agent_label: Option<String>,
    pub runtime: Option<AgentRunListRuntimeSummaryModel>,
    pub children: Vec<AgentRunListChildModel>,
}

#[derive(Debug, Clone)]
pub struct AgentRunListRuntimeSummaryModel {
    pub thread_status: RuntimeThreadStatus,
    pub active_turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunListSubjectModel {
    pub kind: String,
    pub id: Uuid,
    pub label: Option<String>,
}

impl ProjectAgentRunListQuery {
    pub fn new(deps: ProjectAgentRunListQueryDeps) -> Self {
        Self {
            run_repo: deps.run_repo,
            agent_repo: deps.agent_repo,
            lineage_repo: deps.lineage_repo,
            subject_repo: deps.subject_repo,
            project_agent_repo: deps.project_agent_repo,
            runtime: deps.runtime,
        }
    }

    pub async fn list(
        &self,
        input: ProjectAgentRunListInput<'_>,
    ) -> Result<ProjectAgentRunListPage, ApplicationError> {
        let limit = input
            .limit
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT);
        let cursor = input.cursor.and_then(decode_cursor);
        let project_agents = self
            .project_agent_repo
            .list_by_project(input.project_id)
            .await?
            .into_iter()
            .map(|agent| (agent.id, project_agent_label(&agent)))
            .collect::<HashMap<_, _>>();
        let mut runs = self.run_repo.list_by_project(input.project_id).await?;
        runs.sort_by_key(|run| Reverse(run_sort_key(run)));
        if let Some(cursor) = cursor {
            runs.retain(|run| run_sort_key(run) < cursor);
        }

        let mut entries = Vec::new();
        let mut next_cursor = None;
        for (index, run) in runs.iter().enumerate() {
            let agents = self.agent_repo.list_by_run(run.id).await?;
            if agents.is_empty() {
                continue;
            }
            let mut facts = HashMap::new();
            for agent in &agents {
                facts.insert(
                    agent.id,
                    self.agent_facts(run, agent, &project_agents).await?,
                );
            }
            let lineages = self.lineage_repo.list_by_run(run.id).await?;
            let known_agent_ids = agents.iter().map(|agent| agent.id).collect::<HashSet<_>>();
            let (children, child_ids) = lineage_forest(&lineages, &known_agent_ids);
            let mut projected_agent_ids = HashSet::new();

            for agent in agents
                .iter()
                .filter(|agent| !child_ids.contains(&agent.id))
                .chain(agents.iter())
            {
                if !projected_agent_ids.insert(agent.id) {
                    continue;
                }
                let children = project_children(
                    run.id,
                    agent.id,
                    &children,
                    &facts,
                    0,
                    &mut projected_agent_ids,
                );
                let fact = facts.get(&agent.id).ok_or_else(|| {
                    ApplicationError::Internal(format!(
                        "AgentRun list missing facts for LifecycleAgent {}",
                        agent.id
                    ))
                })?;
                entries.push(AgentRunListEntryModel {
                    run_id: run.id,
                    agent_id: agent.id,
                    title: fact.title.clone(),
                    source: fact.source.clone(),
                    lifecycle_status: fact.lifecycle_status.clone(),
                    last_activity_at: fact.last_activity_at.clone(),
                    project_agent_label: fact.project_agent_label.clone(),
                    runtime: fact.runtime.clone(),
                    subject: fact.subject.clone(),
                    subagent_count: projected_descendant_count(&children),
                    children,
                });
            }
            if entries.len() >= limit {
                if index + 1 < runs.len() {
                    next_cursor = Some(encode_cursor(run));
                }
                break;
            }
        }

        Ok(ProjectAgentRunListPage {
            project_id: input.project_id,
            entries,
            next_cursor,
        })
    }

    async fn agent_facts(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        project_agents: &HashMap<Uuid, String>,
    ) -> Result<AgentFacts, ApplicationError> {
        let project_agent_label = agent
            .project_agent_id
            .and_then(|id| project_agents.get(&id).cloned());
        let title = lifecycle_agent_title(agent, project_agents);
        let runtime_view = self
            .runtime
            .inspect(AgentRunRuntimeTarget {
                run_id: run.id,
                agent_id: agent.id,
            })
            .await
            .map_err(|error| {
                ApplicationError::Internal(format!(
                    "AgentRun list runtime inspect failed: run_id={}, agent_id={}: {error}",
                    run.id, agent.id
                ))
            })?;
        let runtime = runtime_view
            .snapshot
            .map(|snapshot| AgentRunListRuntimeSummaryModel {
                thread_status: snapshot.status,
                active_turn_id: snapshot.active_turn_id.map(|id| id.to_string()),
            });
        let subject = self
            .subject_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await?
            .into_iter()
            .next()
            .map(|association| AgentRunListSubjectModel {
                kind: association.subject_kind,
                id: association.subject_id,
                label: subject_label(association.metadata_json.as_ref()),
            });
        Ok(AgentFacts {
            title,
            source: agent.source.as_str().to_string(),
            lifecycle_status: agent.status.clone(),
            last_activity_at: agent.updated_at.to_rfc3339(),
            project_agent_label,
            runtime,
            subject,
        })
    }
}

#[derive(Debug, Clone)]
struct AgentFacts {
    title: String,
    source: String,
    lifecycle_status: String,
    last_activity_at: String,
    project_agent_label: Option<String>,
    runtime: Option<AgentRunListRuntimeSummaryModel>,
    subject: Option<AgentRunListSubjectModel>,
}

fn project_children(
    run_id: Uuid,
    parent_id: Uuid,
    children: &HashMap<Uuid, Vec<Uuid>>,
    facts: &HashMap<Uuid, AgentFacts>,
    depth: usize,
    projected_agent_ids: &mut HashSet<Uuid>,
) -> Vec<AgentRunListChildModel> {
    if depth >= MAX_AGENT_LINEAGE_DEPTH {
        return Vec::new();
    }
    let mut projected = children
        .get(&parent_id)
        .into_iter()
        .flatten()
        .filter_map(|child_id| {
            let fact = facts.get(child_id)?;
            if !projected_agent_ids.insert(*child_id) {
                return None;
            }
            let nested = project_children(
                run_id,
                *child_id,
                children,
                facts,
                depth + 1,
                projected_agent_ids,
            );
            Some(AgentRunListChildModel {
                run_id,
                agent_id: *child_id,
                title: fact.title.clone(),
                source: fact.source.clone(),
                lifecycle_status: fact.lifecycle_status.clone(),
                last_activity_at: fact.last_activity_at.clone(),
                project_agent_label: fact.project_agent_label.clone(),
                runtime: fact.runtime.clone(),
                children: nested,
            })
        })
        .collect::<Vec<_>>();
    projected.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
    projected
}

fn lineage_forest(
    lineages: &[AgentLineage],
    known_agent_ids: &HashSet<Uuid>,
) -> (HashMap<Uuid, Vec<Uuid>>, HashSet<Uuid>) {
    let mut children = HashMap::<Uuid, Vec<Uuid>>::new();
    let mut child_ids = HashSet::new();
    for lineage in lineages {
        if let Some(parent_id) = lineage.parent_agent_id {
            if parent_id == lineage.child_agent_id
                || !known_agent_ids.contains(&parent_id)
                || !known_agent_ids.contains(&lineage.child_agent_id)
            {
                continue;
            }
            children
                .entry(parent_id)
                .or_default()
                .push(lineage.child_agent_id);
            child_ids.insert(lineage.child_agent_id);
        }
    }
    (children, child_ids)
}

fn projected_descendant_count(children: &[AgentRunListChildModel]) -> u32 {
    children.iter().fold(0_u32, |count, child| {
        count
            .saturating_add(1)
            .saturating_add(projected_descendant_count(&child.children))
    })
}

fn subject_label(metadata: Option<&serde_json::Value>) -> Option<String> {
    let object = metadata?.as_object()?;
    ["label", "title", "name"].into_iter().find_map(|key| {
        object
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn run_sort_key(run: &LifecycleRun) -> (i64, Uuid) {
    (run.last_activity_at.timestamp_millis(), run.id)
}

fn encode_cursor(run: &LifecycleRun) -> String {
    URL_SAFE_NO_PAD
        .encode(format!("{}:{}", run.last_activity_at.timestamp_millis(), run.id).as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(i64, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor.as_bytes()).ok()?;
    let raw = String::from_utf8(bytes).ok()?;
    let (millis, id) = raw.split_once(':')?;
    Some((millis.parse().ok()?, Uuid::parse_str(id).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime_contract::{
        OperationReceipt, RuntimeContextView, RuntimeEventStream, RuntimePresentationAppendReceipt,
    };
    use agentdash_application_agentrun::agent_run::{
        AcceptAgentRunMessage, AgentRunMessageAdmission, AgentRunRuntimeError, AgentRunRuntimeView,
        ForkAgentRunRuntime, GuardedAgentRunCommand, ReadAgentRunEvents,
        ResolveAgentRunInteraction, SendAgentRunMessage, SteerAgentRunTurn,
    };
    use agentdash_domain::workflow::AgentSource;
    use agentdash_test_support::workflow::{
        MemoryAgentLineageRepository, MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
        MemoryLifecycleSubjectAssociationRepository, MemoryProjectAgentRepository,
    };
    use async_trait::async_trait;
    use chrono::{Duration, Utc};

    struct FixtureRuntime {
        fail_inspect: bool,
    }

    #[async_trait]
    impl AgentRunRuntime for FixtureRuntime {
        async fn append_presentation(
            &self,
            _: agentdash_application_agentrun::agent_run::AppendAgentRunPresentation,
        ) -> Result<RuntimePresentationAppendReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn inspect(
            &self,
            target: AgentRunRuntimeTarget,
        ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError> {
            if self.fail_inspect {
                return Err(AgentRunRuntimeError::BindingNotFound);
            }
            Ok(AgentRunRuntimeView {
                target,
                binding: None,
                snapshot: None,
                binding_epoch: None,
                recovery: agentdash_application_agentrun::agent_run::AgentRunRuntimeRecoverySummary::Active,
            })
        }

        async fn send_message(
            &self,
            _: SendAgentRunMessage,
        ) -> Result<OperationReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn accept_message(
            &self,
            _: AcceptAgentRunMessage,
        ) -> Result<AgentRunMessageAdmission, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn fork_runtime(
            &self,
            _: ForkAgentRunRuntime,
        ) -> Result<
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBinding,
            AgentRunRuntimeError,
        > {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn compact_context(
            &self,
            _: GuardedAgentRunCommand,
        ) -> Result<OperationReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn steer_active_turn(
            &self,
            _: SteerAgentRunTurn,
        ) -> Result<OperationReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn interrupt_active_turn(
            &self,
            _: GuardedAgentRunCommand,
        ) -> Result<OperationReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn resolve_interaction(
            &self,
            _: ResolveAgentRunInteraction,
        ) -> Result<OperationReceipt, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn read_context(
            &self,
            _: AgentRunRuntimeTarget,
        ) -> Result<RuntimeContextView, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }

        async fn read_events(
            &self,
            _: ReadAgentRunEvents,
        ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError> {
            Err(AgentRunRuntimeError::BindingNotFound)
        }
    }

    struct QueryFixture {
        query: ProjectAgentRunListQuery,
        run_repo: Arc<MemoryLifecycleRunRepository>,
        agent_repo: Arc<MemoryLifecycleAgentRepository>,
        lineage_repo: Arc<MemoryAgentLineageRepository>,
    }

    fn query_fixture(fail_inspect: bool) -> QueryFixture {
        let run_repo = Arc::new(MemoryLifecycleRunRepository::default());
        let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
        let lineage_repo = Arc::new(MemoryAgentLineageRepository::default());
        let query = ProjectAgentRunListQuery::new(ProjectAgentRunListQueryDeps {
            run_repo: run_repo.clone(),
            agent_repo: agent_repo.clone(),
            lineage_repo: lineage_repo.clone(),
            subject_repo: Arc::new(MemoryLifecycleSubjectAssociationRepository::default()),
            project_agent_repo: Arc::new(MemoryProjectAgentRepository::default()),
            runtime: Arc::new(FixtureRuntime { fail_inspect }),
        });
        QueryFixture {
            query,
            run_repo,
            agent_repo,
            lineage_repo,
        }
    }

    #[test]
    fn lineage_forest_preserves_parent_child_structure() {
        let run_id = Uuid::new_v4();
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let grandchild = Uuid::new_v4();
        let lineages = vec![
            AgentLineage::new(run_id, Some(root), child, "subagent", None, None),
            AgentLineage::new(run_id, Some(child), grandchild, "delegate", None, None),
        ];
        let known_agent_ids = HashSet::from([root, child, grandchild]);
        let (children, child_ids) = lineage_forest(&lineages, &known_agent_ids);
        assert_eq!(children.get(&root), Some(&vec![child]));
        assert!(child_ids.contains(&grandchild));
        assert_eq!(children.get(&child), Some(&vec![grandchild]));
    }

    #[test]
    fn cursor_round_trip_uses_run_activity_and_id() {
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let cursor = encode_cursor(&run);
        assert!(!cursor.contains(':'), "Main cursor must remain opaque");
        assert_eq!(decode_cursor(&cursor).expect("cursor"), run_sort_key(&run));
        assert_eq!(decode_cursor("not-a-main-cursor"), None);
    }

    #[test]
    fn child_projection_sorts_every_level_by_main_last_activity_order() {
        let run_id = Uuid::new_v4();
        let root = Uuid::new_v4();
        let older = Uuid::new_v4();
        let newer = Uuid::new_v4();
        let facts = HashMap::from([
            (
                older,
                AgentFacts {
                    title: "Older".to_string(),
                    source: "subagent".to_string(),
                    lifecycle_status: "completed".to_string(),
                    last_activity_at: "2026-07-10T00:01:00Z".to_string(),
                    project_agent_label: None,
                    runtime: None,
                    subject: None,
                },
            ),
            (
                newer,
                AgentFacts {
                    title: "Newer".to_string(),
                    source: "subagent".to_string(),
                    lifecycle_status: "running".to_string(),
                    last_activity_at: "2026-07-10T00:02:00Z".to_string(),
                    project_agent_label: None,
                    runtime: None,
                    subject: None,
                },
            ),
        ]);
        let children = HashMap::from([(root, vec![older, newer])]);
        let mut projected_agent_ids = HashSet::from([root]);

        let projected =
            project_children(run_id, root, &children, &facts, 0, &mut projected_agent_ids);

        assert_eq!(
            projected
                .iter()
                .map(|child| child.agent_id)
                .collect::<Vec<_>>(),
            vec![newer, older]
        );
    }

    #[test]
    fn child_projection_stops_a_lineage_cycle() {
        let run_id = Uuid::new_v4();
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let facts = [root, child]
            .into_iter()
            .map(|id| {
                (
                    id,
                    AgentFacts {
                        title: id.to_string(),
                        source: "subagent".to_string(),
                        lifecycle_status: "active".to_string(),
                        last_activity_at: "2026-07-12T00:00:00Z".to_string(),
                        project_agent_label: None,
                        runtime: None,
                        subject: None,
                    },
                )
            })
            .collect();
        let children = HashMap::from([(root, vec![child]), (child, vec![root])]);

        let mut projected_agent_ids = HashSet::from([root]);
        let projected =
            project_children(run_id, root, &children, &facts, 0, &mut projected_agent_ids);

        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].agent_id, child);
        assert!(projected[0].children.is_empty());
        assert_eq!(projected_descendant_count(&projected), 1);
    }

    #[tokio::test]
    async fn query_uses_run_activity_keyset_and_keeps_unbound_runtime_optional() {
        let fixture = query_fixture(false);
        let project_id = Uuid::new_v4();
        let mut older_run = LifecycleRun::new_plain(project_id);
        older_run.last_activity_at = Utc::now() - Duration::minutes(2);
        let mut newer_run = LifecycleRun::new_plain(project_id);
        newer_run.last_activity_at = Utc::now() - Duration::minutes(1);
        let older_agent =
            LifecycleAgent::new_root(older_run.id, project_id, AgentSource::ProjectAgent);
        let newer_agent =
            LifecycleAgent::new_root(newer_run.id, project_id, AgentSource::ProjectAgent);
        fixture
            .run_repo
            .create(&older_run)
            .await
            .expect("older run");
        fixture
            .run_repo
            .create(&newer_run)
            .await
            .expect("newer run");
        fixture
            .agent_repo
            .create(&older_agent)
            .await
            .expect("older agent");
        fixture
            .agent_repo
            .create(&newer_agent)
            .await
            .expect("newer agent");

        let first = fixture
            .query
            .list(ProjectAgentRunListInput {
                project_id,
                limit: Some(1),
                cursor: None,
            })
            .await
            .expect("first page");
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.entries[0].run_id, newer_run.id);
        assert!(first.entries[0].runtime.is_none());
        let cursor = first.next_cursor.expect("next cursor");

        let second = fixture
            .query
            .list(ProjectAgentRunListInput {
                project_id,
                limit: Some(1),
                cursor: Some(&cursor),
            })
            .await
            .expect("second page");
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].run_id, older_run.id);
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn query_projects_every_agent_once_when_lineage_contains_a_cycle() {
        let fixture = query_fixture(false);
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let root = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let child = LifecycleAgent::new_root(run.id, project_id, AgentSource::Subagent);
        fixture.run_repo.create(&run).await.expect("run");
        fixture.agent_repo.create(&root).await.expect("root");
        fixture.agent_repo.create(&child).await.expect("child");
        fixture
            .lineage_repo
            .create(&AgentLineage::new(
                run.id,
                Some(root.id),
                child.id,
                "subagent",
                None,
                None,
            ))
            .await
            .expect("root to child");
        fixture
            .lineage_repo
            .create(&AgentLineage::new(
                run.id,
                Some(child.id),
                root.id,
                "invalid_cycle",
                None,
                None,
            ))
            .await
            .expect("cycle edge");

        let page = fixture
            .query
            .list(ProjectAgentRunListInput {
                project_id,
                limit: None,
                cursor: None,
            })
            .await
            .expect("cycle-safe page");

        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].agent_id, root.id);
        assert_eq!(page.entries[0].children.len(), 1);
        assert_eq!(page.entries[0].children[0].agent_id, child.id);
        assert!(page.entries[0].children[0].children.is_empty());
        assert_eq!(page.entries[0].subagent_count, 1);
    }

    #[tokio::test]
    async fn query_reports_runtime_inspect_failure_with_product_coordinates() {
        let fixture = query_fixture(true);
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        fixture.run_repo.create(&run).await.expect("run");
        fixture.agent_repo.create(&agent).await.expect("agent");

        let error = fixture
            .query
            .list(ProjectAgentRunListInput {
                project_id,
                limit: None,
                cursor: None,
            })
            .await
            .expect_err("runtime inspect failure must fail the list");
        let message = error.to_string();
        assert!(message.contains(&run.id.to_string()));
        assert!(message.contains(&agent.id.to_string()));
    }
}
