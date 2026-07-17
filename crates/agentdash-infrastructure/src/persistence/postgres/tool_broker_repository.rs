use agentdash_agent_runtime::{
    ToolBrokerCall, ToolBrokerCallStatus, ToolBrokerRepository, ToolBrokerStoreError,
    ToolBrokerTransition, ToolCallAdmission, ToolExecutionClaim,
};
use agentdash_agent_runtime_contract::RuntimeItemId;
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PostgresToolBrokerRepository {
    pool: PgPool,
}

impl PostgresToolBrokerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ToolBrokerRepository for PostgresToolBrokerRepository {
    async fn load(
        &self,
        item_id: &RuntimeItemId,
    ) -> Result<Option<ToolBrokerCall>, ToolBrokerStoreError> {
        let row = sqlx::query("SELECT record FROM agent_runtime_tool_call WHERE item_id=$1")
            .bind(item_id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(store_error)?;
        row.map(|row| decode_call(row.get("record"))).transpose()
    }

    async fn accept(
        &self,
        call: ToolBrokerCall,
    ) -> Result<ToolCallAdmission, ToolBrokerStoreError> {
        let record = serde_json::to_value(&call).map_err(json_error)?;
        let result = sqlx::query(
            "INSERT INTO agent_runtime_tool_call (item_id,thread_id,turn_id,binding_id,binding_generation,tool_set_revision,tool_name,invocation_digest,capability_key,tool_path,channel,status,pending_interaction_id,record) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (item_id) DO NOTHING",
        )
        .bind(call.invocation.coordinates.item_id.as_str())
        .bind(call.invocation.coordinates.thread_id.as_str())
        .bind(call.invocation.coordinates.turn_id.as_str())
        .bind(call.invocation.coordinates.binding_id.as_str())
        .bind(to_i64(call.invocation.coordinates.binding_generation.0)?)
        .bind(to_i64(call.invocation.coordinates.tool_set_revision.0)?)
        .bind(&call.invocation.tool_name)
        .bind(&call.invocation_digest)
        .bind(&call.capability_key)
        .bind(&call.tool_path)
        .bind(channel_key(call.channel))
        .bind(status_key(call.status))
        .bind(
            call.pending_interaction_id
                .as_ref()
                .map(|interaction_id| interaction_id.as_str()),
        )
        .bind(record)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(if result.rows_affected() == 1 {
            ToolCallAdmission::Accepted
        } else {
            ToolCallAdmission::Existing
        })
    }

    async fn recoverable(&self) -> Result<Vec<ToolBrokerCall>, ToolBrokerStoreError> {
        let rows = sqlx::query(
            "SELECT record FROM agent_runtime_tool_call WHERE status='accepted' ORDER BY updated_at,item_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;
        rows.into_iter()
            .map(|row| decode_call(row.get("record")))
            .collect()
    }

    async fn claim_execution(
        &self,
        item_id: &RuntimeItemId,
        effective_arguments: serde_json::Value,
    ) -> Result<ToolExecutionClaim, ToolBrokerStoreError> {
        let mut transaction = self.pool.begin().await.map_err(store_error)?;
        let row =
            sqlx::query("SELECT record FROM agent_runtime_tool_call WHERE item_id=$1 FOR UPDATE")
                .bind(item_id.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(store_error)?
                .ok_or(ToolBrokerStoreError::Conflict)?;
        let mut call = decode_call(row.get("record"))?;
        let claim = match call.status {
            ToolBrokerCallStatus::Accepted => {
                call.status = ToolBrokerCallStatus::Running;
                call.effective_arguments = Some(effective_arguments);
                call.pending_interaction_id = None;
                ToolExecutionClaim::Acquired(call.clone())
            }
            ToolBrokerCallStatus::AwaitingApproval
                if call.effective_arguments.as_ref() == Some(&effective_arguments) =>
            {
                call.status = ToolBrokerCallStatus::Running;
                call.pending_interaction_id = None;
                ToolExecutionClaim::Acquired(call.clone())
            }
            ToolBrokerCallStatus::Running
                if call.effective_arguments.as_ref() == Some(&effective_arguments) =>
            {
                transaction.commit().await.map_err(store_error)?;
                return Ok(ToolExecutionClaim::InProgress(call));
            }
            status
                if status.is_terminal()
                    && call.effective_arguments.as_ref() == Some(&effective_arguments) =>
            {
                transaction.commit().await.map_err(store_error)?;
                return Ok(ToolExecutionClaim::Terminal(call));
            }
            _ => return Err(ToolBrokerStoreError::Conflict),
        };
        let record = serde_json::to_value(&call).map_err(json_error)?;
        sqlx::query(
            "UPDATE agent_runtime_tool_call SET status='running',pending_interaction_id=NULL,record=$2,updated_at=now() WHERE item_id=$1",
        )
        .bind(item_id.as_str())
        .bind(record)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        transaction.commit().await.map_err(store_error)?;
        Ok(claim)
    }

    async fn transition(
        &self,
        item_id: &RuntimeItemId,
        transition: ToolBrokerTransition,
    ) -> Result<ToolBrokerCall, ToolBrokerStoreError> {
        let ToolBrokerTransition {
            expected,
            next,
            effective_arguments,
            pending_interaction_id,
            result,
            message,
        } = transition;
        let mut transaction = self.pool.begin().await.map_err(store_error)?;
        let row =
            sqlx::query("SELECT record FROM agent_runtime_tool_call WHERE item_id=$1 FOR UPDATE")
                .bind(item_id.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(store_error)?
                .ok_or(ToolBrokerStoreError::Conflict)?;
        let mut call = decode_call(row.get("record"))?;
        if call.status == next
            && call.effective_arguments == effective_arguments
            && call.pending_interaction_id == pending_interaction_id
            && call.result == result
            && call.terminal_message == message
        {
            transaction.commit().await.map_err(store_error)?;
            return Ok(call);
        }
        if !expected.contains(&call.status)
            || !valid_transition(call.status, next)
            || (call.status != ToolBrokerCallStatus::Accepted
                && call.effective_arguments != effective_arguments)
        {
            return Err(ToolBrokerStoreError::Conflict);
        }
        call.status = next;
        call.effective_arguments = effective_arguments;
        call.pending_interaction_id = pending_interaction_id;
        call.result = result;
        call.terminal_message = message;
        let record = serde_json::to_value(&call).map_err(json_error)?;
        sqlx::query(
            "UPDATE agent_runtime_tool_call SET status=$2,pending_interaction_id=$3,record=$4,updated_at=now() WHERE item_id=$1",
        )
        .bind(item_id.as_str())
        .bind(status_key(next))
        .bind(
            call.pending_interaction_id
                .as_ref()
                .map(|interaction_id| interaction_id.as_str()),
        )
        .bind(record)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        transaction.commit().await.map_err(store_error)?;
        Ok(call)
    }
}

fn decode_call(value: serde_json::Value) -> Result<ToolBrokerCall, ToolBrokerStoreError> {
    serde_json::from_value(value).map_err(json_error)
}

fn to_i64(value: u64) -> Result<i64, ToolBrokerStoreError> {
    i64::try_from(value)
        .map_err(|_| ToolBrokerStoreError::Unavailable("revision exceeds PostgreSQL bigint".into()))
}

fn status_key(status: ToolBrokerCallStatus) -> &'static str {
    match status {
        ToolBrokerCallStatus::Accepted => "accepted",
        ToolBrokerCallStatus::AwaitingApproval => "awaiting_approval",
        ToolBrokerCallStatus::Running => "running",
        ToolBrokerCallStatus::Completed => "completed",
        ToolBrokerCallStatus::Failed => "failed",
        ToolBrokerCallStatus::Cancelled => "cancelled",
        ToolBrokerCallStatus::TimedOut => "timed_out",
    }
}

fn channel_key(channel: agentdash_agent_runtime_contract::ToolChannel) -> &'static str {
    match channel {
        agentdash_agent_runtime_contract::ToolChannel::DirectCallback => "direct_callback",
        agentdash_agent_runtime_contract::ToolChannel::McpFacade => "mcp_facade",
        agentdash_agent_runtime_contract::ToolChannel::DriverNative => "driver_native",
    }
}

fn store_error(error: sqlx::Error) -> ToolBrokerStoreError {
    ToolBrokerStoreError::Unavailable(error.to_string())
}

fn json_error(error: serde_json::Error) -> ToolBrokerStoreError {
    ToolBrokerStoreError::Unavailable(error.to_string())
}

fn valid_transition(current: ToolBrokerCallStatus, next: ToolBrokerCallStatus) -> bool {
    use ToolBrokerCallStatus::{
        Accepted, AwaitingApproval, Cancelled, Completed, Failed, Running, TimedOut,
    };
    matches!(
        (current, next),
        (Accepted, AwaitingApproval | Running | Failed | Cancelled)
            | (AwaitingApproval, Running | Failed | Cancelled)
            | (Running, Completed | Failed | Cancelled | TimedOut)
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::PostgresToolBrokerRepository;
    use agentdash_agent_runtime::{
        ContributionMeta, ContributionRequirement, SurfaceSourceRef, ToolBrokerCall,
        ToolBrokerCallStatus, ToolBrokerInvocation, ToolBrokerRepository, ToolBrokerResult,
        ToolBrokerTransition, ToolCallAdmission, ToolCallCoordinates, ToolContribution,
        ToolExecutionClaim,
    };
    use agentdash_agent_runtime_contract::{
        ConfigurationBoundary, RuntimeBindingId, RuntimeDriverGeneration, RuntimeInteractionId,
        RuntimeItemId, RuntimeThreadId, RuntimeTurnId, ToolChannel, ToolPresentationEmitter,
        ToolProtocolProjection, ToolSetRevision,
    };

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid id")
    }

    async fn test_pool() -> (
        sqlx::PgPool,
        Option<crate::postgres_runtime::PostgresRuntime>,
    ) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("tool broker postgres")
                    .await
                    .expect("configured postgres test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tool-broker-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "tool-broker-tests",
            8,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL");
        let database_name = format!("tool_broker_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .expect("connect database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("run migrations");
        (pool, Some(runtime))
    }

    async fn seed_coordinates(pool: &sqlx::PgPool) {
        sqlx::query("INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ('binding-tool',3,'profile')")
            .execute(pool)
            .await
            .expect("binding");
        sqlx::query("INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ('binding-tool','source-tool','thread-tool')")
            .execute(pool)
            .await
            .expect("source coordinate");
        sqlx::query("INSERT INTO agent_runtime_thread (id,revision,next_event_sequence,next_operation_sequence,status,binding_id,driver_generation,source_thread_id,profile_digest,context_revision,settings_revision,tool_set_revision,projection) VALUES ('thread-tool',0,0,0,'active','binding-tool',3,'source-tool','profile',0,0,4,'{}')")
            .execute(pool)
            .await
            .expect("thread");
        sqlx::query("INSERT INTO agent_runtime_turn (id,thread_id,phase,state) VALUES ('turn-tool','thread-tool','active','{}')")
            .execute(pool)
            .await
            .expect("turn");
        sqlx::query("INSERT INTO agent_runtime_item (id,thread_id,turn_id,sort_order,phase,state) VALUES ('item-tool','thread-tool','turn-tool',0,'active','{}')")
            .execute(pool)
            .await
            .expect("item");
        sqlx::query("INSERT INTO agent_runtime_item (id,thread_id,turn_id,sort_order,phase,state) VALUES ('item-approval','thread-tool','turn-tool',1,'active','{}')")
            .execute(pool)
            .await
            .expect("approval item");
        sqlx::query("INSERT INTO agent_runtime_item (id,thread_id,turn_id,sort_order,phase,state) VALUES ('item-concurrent','thread-tool','turn-tool',2,'active','{}')")
            .execute(pool)
            .await
            .expect("concurrent item");
    }

    fn call() -> ToolBrokerCall {
        ToolBrokerCall {
            invocation: ToolBrokerInvocation {
                coordinates: ToolCallCoordinates {
                    thread_id: id::<RuntimeThreadId>("thread-tool"),
                    turn_id: id::<RuntimeTurnId>("turn-tool"),
                    item_id: id::<RuntimeItemId>("item-tool"),
                    presentation_item_id: id("turn_001:tool_001"),
                    source_thread_id: id("source-tool"),
                    source_turn_id: id("source-turn-tool"),
                    source_item_id: id("source-item-tool"),
                    binding_id: id::<RuntimeBindingId>("binding-tool"),
                    binding_generation: RuntimeDriverGeneration(3),
                    tool_set_revision: ToolSetRevision(4),
                },
                tool_name: "workspace_read".to_string(),
                arguments: serde_json::json!({"path":"README.md"}),
                timeout_ms: 1000,
            },
            invocation_digest: "sha256:invocation".to_string(),
            capability_key: "file_read".to_string(),
            tool_path: "file_read::workspace_read".to_string(),
            tool: ToolContribution {
                meta: ContributionMeta {
                    key: "tool:file_read:workspace_read".to_string(),
                    source: SurfaceSourceRef {
                        layer: "agent_frame".to_string(),
                        key: "test:tool-broker-repository".to_string(),
                    },
                    priority: 0,
                    requirement: ContributionRequirement::Required,
                },
                runtime_name: "workspace_read".to_string(),
                description: "Read a workspace file".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
                capability_key: "file_read".to_string(),
                tool_path: "file_read::workspace_read".to_string(),
                allowed_channels: [ToolChannel::DirectCallback].into(),
                configuration_boundary: ConfigurationBoundary::Binding,
                protocol_projection: ToolProtocolProjection::FsRead,
                presentation_emitter: ToolPresentationEmitter::ToolBroker,
                parity_fixture_id: "main_tool_workspace_read_lifecycle".into(),
            },
            channel: ToolChannel::DirectCallback,
            status: ToolBrokerCallStatus::Accepted,
            effective_arguments: None,
            pending_interaction_id: None,
            result: None,
            terminal_message: None,
        }
    }

    #[tokio::test]
    async fn postgres_store_accepts_once_and_persists_terminal() {
        let (pool, _runtime) = test_pool().await;
        seed_coordinates(&pool).await;
        let repository = PostgresToolBrokerRepository::new(pool.clone());

        assert_eq!(
            repository.accept(call()).await.expect("accept"),
            ToolCallAdmission::Accepted
        );
        assert_eq!(
            repository.accept(call()).await.expect("duplicate"),
            ToolCallAdmission::Existing
        );
        assert_eq!(
            repository
                .transition(
                    &id("item-tool"),
                    ToolBrokerTransition {
                        expected: vec![ToolBrokerCallStatus::Accepted],
                        next: ToolBrokerCallStatus::Completed,
                        effective_arguments: Some(serde_json::json!({"path":"README.md"})),
                        pending_interaction_id: None,
                        result: Some(ToolBrokerResult {
                            output: serde_json::json!({"content":"invalid"}),
                            is_error: false,
                        }),
                        message: None,
                    },
                )
                .await,
            Err(agentdash_agent_runtime::ToolBrokerStoreError::Conflict)
        );
        let result = ToolBrokerResult {
            output: serde_json::json!({"content":"ok"}),
            is_error: false,
        };
        assert_eq!(
            repository.recoverable().await.expect("recoverable").len(),
            1
        );
        let item_id: RuntimeItemId = id("item-tool");
        let effective_arguments = serde_json::json!({"path":"README.md"});
        let (left, right) = tokio::join!(
            repository.claim_execution(&item_id, effective_arguments.clone()),
            repository.claim_execution(&item_id, effective_arguments.clone())
        );
        assert!(
            matches!(&left, Ok(ToolExecutionClaim::Acquired(_)))
                ^ matches!(&right, Ok(ToolExecutionClaim::Acquired(_)))
        );
        assert!(
            matches!(&left, Ok(ToolExecutionClaim::InProgress(_)))
                ^ matches!(&right, Ok(ToolExecutionClaim::InProgress(_)))
        );
        assert!(
            repository
                .recoverable()
                .await
                .expect("running recovery scan")
                .is_empty(),
            "a persisted Running call must not be replayed after restart"
        );
        let terminal = repository
            .transition(
                &id("item-tool"),
                ToolBrokerTransition {
                    expected: vec![ToolBrokerCallStatus::Running],
                    next: ToolBrokerCallStatus::Completed,
                    effective_arguments: Some(serde_json::json!({"path":"README.md"})),
                    pending_interaction_id: None,
                    result: Some(result.clone()),
                    message: None,
                },
            )
            .await
            .expect("terminal");
        assert_eq!(terminal.status, ToolBrokerCallStatus::Completed);
        assert_eq!(terminal.result, Some(result));
        assert_eq!(
            repository
                .load(&id("item-tool"))
                .await
                .expect("load")
                .expect("call")
                .status,
            ToolBrokerCallStatus::Completed
        );
        assert!(
            repository
                .recoverable()
                .await
                .expect("terminal recovery scan")
                .is_empty()
        );

        let mut concurrent_call = call();
        concurrent_call.invocation.coordinates.item_id = id("item-concurrent");
        repository
            .accept(concurrent_call)
            .await
            .expect("accept concurrent call");
        repository
            .transition(
                &id("item-concurrent"),
                ToolBrokerTransition {
                    expected: vec![ToolBrokerCallStatus::Accepted],
                    next: ToolBrokerCallStatus::Running,
                    effective_arguments: Some(serde_json::json!({"path":"README.md"})),
                    pending_interaction_id: None,
                    result: None,
                    message: None,
                },
            )
            .await
            .expect("running concurrent call");
        let completed = ToolBrokerTransition {
            expected: vec![ToolBrokerCallStatus::Running],
            next: ToolBrokerCallStatus::Completed,
            effective_arguments: Some(serde_json::json!({"path":"README.md"})),
            pending_interaction_id: None,
            result: Some(ToolBrokerResult {
                output: serde_json::json!({"content":"completed"}),
                is_error: false,
            }),
            message: None,
        };
        let failed = ToolBrokerTransition {
            expected: vec![ToolBrokerCallStatus::Running],
            next: ToolBrokerCallStatus::Failed,
            effective_arguments: Some(serde_json::json!({"path":"README.md"})),
            pending_interaction_id: None,
            result: Some(ToolBrokerResult {
                output: serde_json::json!({"error":"failed"}),
                is_error: true,
            }),
            message: Some("failed".to_string()),
        };
        let concurrent_item_id: RuntimeItemId = id("item-concurrent");
        let (left, right) = tokio::join!(
            repository.transition(&concurrent_item_id, completed),
            repository.transition(&concurrent_item_id, failed)
        );
        assert_ne!(left.is_ok(), right.is_ok());
        assert!(
            repository
                .load(&id("item-concurrent"))
                .await
                .expect("load concurrent terminal")
                .expect("concurrent call")
                .status
                .is_terminal()
        );

        let mut approval_call = call();
        approval_call.invocation.coordinates.item_id = id("item-approval");
        repository
            .accept(approval_call)
            .await
            .expect("accept approval call");
        let interaction_id: RuntimeInteractionId = id("interaction-tool-approval");
        assert!(matches!(
            repository
                .transition(
                    &id("item-approval"),
                    ToolBrokerTransition {
                        expected: vec![ToolBrokerCallStatus::Accepted],
                        next: ToolBrokerCallStatus::AwaitingApproval,
                        effective_arguments: Some(serde_json::json!({"path":"README.md"})),
                        pending_interaction_id: Some(interaction_id.clone()),
                        result: None,
                        message: Some("approval required".to_string()),
                    },
                )
                .await,
            Err(agentdash_agent_runtime::ToolBrokerStoreError::Unavailable(
                _
            ))
        ));
        assert_eq!(
            repository
                .load(&id("item-approval"))
                .await
                .expect("load rolled back approval")
                .expect("approval call")
                .status,
            ToolBrokerCallStatus::Accepted
        );
        sqlx::query("INSERT INTO agent_runtime_interaction (id,thread_id,turn_id,phase,state) VALUES ($1,'thread-tool','turn-tool','active','{}')")
            .bind(interaction_id.as_str())
            .execute(&pool)
            .await
            .expect("canonical approval interaction");
        let awaiting = repository
            .transition(
                &id("item-approval"),
                ToolBrokerTransition {
                    expected: vec![ToolBrokerCallStatus::Accepted],
                    next: ToolBrokerCallStatus::AwaitingApproval,
                    effective_arguments: Some(serde_json::json!({"path":"README.md"})),
                    pending_interaction_id: Some(interaction_id.clone()),
                    result: None,
                    message: Some("approval required".to_string()),
                },
            )
            .await
            .expect("reference canonical approval interaction");
        assert_eq!(awaiting.pending_interaction_id, Some(interaction_id));
    }
}
