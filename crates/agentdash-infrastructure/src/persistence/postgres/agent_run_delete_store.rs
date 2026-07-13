use agentdash_application_ports::agent_run_delete::{
    AgentRunDeleteStore, DeleteAgentRunCommand, DeleteAgentRunError, DeleteAgentRunOutcome,
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

pub struct PostgresAgentRunDeleteStore {
    pool: PgPool,
}

impl PostgresAgentRunDeleteStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn persistence(error: sqlx::Error) -> DeleteAgentRunError {
    DeleteAgentRunError::Persistence(error.to_string())
}

#[async_trait]
impl AgentRunDeleteStore for PostgresAgentRunDeleteStore {
    async fn delete(
        &self,
        command: DeleteAgentRunCommand,
    ) -> Result<DeleteAgentRunOutcome, DeleteAgentRunError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        sqlx::query("SET CONSTRAINTS ALL DEFERRED")
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;

        let run =
            sqlx::query("SELECT project_id,status FROM lifecycle_runs WHERE id=$1 FOR UPDATE")
                .bind(command.run_id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(persistence)?
                .ok_or(DeleteAgentRunError::NotFound {
                    run_id: command.run_id,
                })?;
        let project_id: String = run.try_get("project_id").map_err(persistence)?;
        if project_id != command.project_id.to_string() {
            return Err(DeleteAgentRunError::NotFound {
                run_id: command.run_id,
            });
        }
        let lifecycle_status: String = run.try_get("status").map_err(persistence)?;
        let lifecycle_agent_statuses = sqlx::query_scalar::<_, String>(
            "SELECT status FROM lifecycle_agents WHERE run_id=$1 FOR UPDATE",
        )
        .bind(command.run_id.to_string())
        .fetch_all(&mut *tx)
        .await
        .map_err(persistence)?;

        let runtime_thread_ids = sqlx::query_scalar::<_, String>(
            "SELECT runtime_thread_id FROM agent_run_runtime_thread_anchor \
             WHERE run_id=$1 FOR UPDATE",
        )
        .bind(command.run_id.to_string())
        .fetch_all(&mut *tx)
        .await
        .map_err(persistence)?;
        let runtime_rows = if runtime_thread_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query(
                "SELECT id,active_turn_id,binding_id FROM agent_runtime_thread \
                 WHERE id=ANY($1) FOR UPDATE",
            )
            .bind(&runtime_thread_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(persistence)?
        };
        if lifecycle_status == "running"
            || lifecycle_agent_statuses
                .iter()
                .any(|status| status == "running" || status == "cancelling")
            || runtime_rows.iter().any(|row| {
                row.try_get::<Option<String>, _>("active_turn_id")
                    .ok()
                    .flatten()
                    .is_some()
            })
        {
            return Err(DeleteAgentRunError::RuntimeActive {
                run_id: command.run_id,
            });
        }

        let mut runtime_binding_ids = runtime_rows
            .iter()
            .filter_map(|row| {
                row.try_get::<Option<String>, _>("binding_id")
                    .ok()
                    .flatten()
            })
            .collect::<Vec<_>>();
        runtime_binding_ids.extend(
            sqlx::query_scalar::<_, String>(
                "SELECT runtime_binding_id FROM agent_run_runtime_binding_lineage WHERE run_id=$1 \
                 UNION SELECT bootstrap_runtime_binding_id FROM agent_run_runtime_thread_anchor WHERE run_id=$1 \
                 UNION SELECT proposed_binding_id FROM agent_run_runtime_recovery_intent WHERE run_id=$1",
            )
            .bind(command.run_id.to_string())
            .fetch_all(&mut *tx)
            .await
            .map_err(persistence)?,
        );
        runtime_binding_ids.sort();
        runtime_binding_ids.dedup();

        if !runtime_thread_ids.is_empty() {
            sqlx::query(
                "DELETE FROM permission_grants WHERE source_runtime_operation_id IN \
                 (SELECT id FROM agent_runtime_operation WHERE thread_id=ANY($1))",
            )
            .bind(&runtime_thread_ids)
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
            sqlx::query(
                "UPDATE agent_runtime_thread SET active_checkpoint_id=NULL WHERE id=ANY($1)",
            )
            .bind(&runtime_thread_ids)
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
            for table in [
                "agent_context_activation_dispatch",
                "agent_context_head",
                "agent_context_activation",
                "agent_context_candidate",
                "agent_context_preparation",
                "agent_context_checkpoint",
                "agent_runtime_hook_effect",
                "agent_runtime_hook_run",
                "agent_runtime_hook_plan",
                "agent_runtime_tool_call",
            ] {
                sqlx::query(&format!("DELETE FROM {table} WHERE thread_id=ANY($1)"))
                    .bind(&runtime_thread_ids)
                    .execute(&mut *tx)
                    .await
                    .map_err(persistence)?;
            }
        }

        sqlx::query("DELETE FROM agent_run_runtime_binding_lineage WHERE run_id=$1")
            .bind(command.run_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        sqlx::query("DELETE FROM agent_run_runtime_recovery_intent WHERE run_id=$1")
            .bind(command.run_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        sqlx::query("DELETE FROM agent_run_runtime_thread_anchor WHERE run_id=$1")
            .bind(command.run_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;

        if !runtime_thread_ids.is_empty() {
            sqlx::query("DELETE FROM agent_runtime_quarantine WHERE thread_id=ANY($1)")
                .bind(&runtime_thread_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            sqlx::query("DELETE FROM agent_runtime_thread WHERE id=ANY($1)")
                .bind(&runtime_thread_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
        }
        if !runtime_binding_ids.is_empty() {
            sqlx::query("DELETE FROM agent_runtime_surface_snapshot WHERE binding_id=ANY($1)")
                .bind(&runtime_binding_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            sqlx::query("DELETE FROM agent_runtime_quarantine WHERE binding_id=ANY($1)")
                .bind(&runtime_binding_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            sqlx::query("DELETE FROM agent_runtime_host_binding WHERE binding_id=ANY($1)")
                .bind(&runtime_binding_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
            sqlx::query("DELETE FROM agent_runtime_binding WHERE id=ANY($1)")
                .bind(&runtime_binding_ids)
                .execute(&mut *tx)
                .await
                .map_err(persistence)?;
        }

        let deleted = sqlx::query("DELETE FROM lifecycle_runs WHERE id=$1 AND project_id=$2")
            .bind(command.run_id.to_string())
            .bind(command.project_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(persistence)?;
        if deleted.rows_affected() != 1 {
            return Err(DeleteAgentRunError::NotFound {
                run_id: command.run_id,
            });
        }
        tx.commit().await.map_err(persistence)?;
        Ok(DeleteAgentRunOutcome {
            project_id: command.project_id,
            run_id: command.run_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use agentdash_application_ports::agent_run_delete::AgentRunDeleteStore;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn deletes_complete_canonical_runtime_owner_graph_and_rejects_active_turn() {
        let (pool, _runtime) = test_pool().await;
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let suffix = Uuid::new_v4().simple().to_string();
        let thread_id = format!("delete-thread-{suffix}");
        let binding_id = format!("delete-binding-{suffix}");
        let source_thread_id = format!("delete-source-{suffix}");
        let service_id = format!("delete-service-{suffix}");
        let offer_id = format!("delete-offer-{suffix}");
        let profile_digest = format!("delete-profile-{suffix}");
        let mut seed = pool.begin().await.expect("seed transaction");
        sqlx::query("SET CONSTRAINTS ALL DEFERRED")
            .execute(&mut *seed)
            .await
            .expect("defer constraints");
        sqlx::query("INSERT INTO projects (id,name,created_at,updated_at) VALUES ($1,'Delete test',now(),now())")
            .bind(project_id.to_string()).execute(&mut *seed).await.expect("project");
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,topology,status,created_at,updated_at,last_activity_at) VALUES ($1,$2,'plain','completed',now(),now(),now())")
            .bind(run_id.to_string()).bind(project_id.to_string()).execute(&mut *seed).await.expect("run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,source,status,bootstrap_status) VALUES ($1,$2,$3,'primary','active','not_applicable')")
            .bind(agent_id.to_string()).bind(run_id.to_string()).bind(project_id.to_string()).execute(&mut *seed).await.expect("agent");
        sqlx::query("INSERT INTO agent_runtime_service_instance (id,definition_id,definition_build_digest,revision,config,credentials,placement,desired_state,observed_state,active_generation) VALUES ($1,'delete-definition','sha256:delete',1,'{}','{}','{}','active','{}',1)")
            .bind(&service_id).execute(&mut *seed).await.expect("service");
        sqlx::query("INSERT INTO agent_runtime_service_instance_revision (service_instance_id,revision,instance_snapshot) VALUES ($1,1,'{}')")
            .bind(&service_id).execute(&mut *seed).await.expect("service revision");
        sqlx::query("INSERT INTO agent_runtime_service_activation (service_instance_id,instance_revision,driver_generation,protocol_revision,effective_profile,profile_digest,conformance_evidence,instance_snapshot) VALUES ($1,1,1,1,'{}',$2,'{}','{}')")
            .bind(&service_id).bind(&profile_digest).execute(&mut *seed).await.expect("activation");
        sqlx::query("INSERT INTO agent_runtime_offer (id,service_instance_id,instance_revision,driver_generation,profile_digest,available,offer) VALUES ($1,$2,1,1,$3,true,'{}')")
            .bind(&offer_id).bind(&service_id).bind(&profile_digest).execute(&mut *seed).await.expect("offer");
        sqlx::query("INSERT INTO agent_runtime_binding (id,driver_generation,profile_digest) VALUES ($1,1,$2)")
            .bind(&binding_id).bind(&profile_digest).execute(&mut *seed).await.expect("binding");
        sqlx::query("INSERT INTO agent_runtime_source_coordinate (binding_id,source_thread_id,thread_id) VALUES ($1,$2,$3)")
            .bind(&binding_id).bind(&source_thread_id).bind(&thread_id).execute(&mut *seed).await.expect("coordinate");
        sqlx::query("INSERT INTO agent_runtime_thread (id,revision,next_event_sequence,next_operation_sequence,status,active_turn_id,binding_id,driver_generation,source_thread_id,profile_digest,context_revision,settings_revision,tool_set_revision,projection) VALUES ($1,0,0,0,'active','turn-active',$2,1,$3,$4,0,0,0,'{}')")
            .bind(&thread_id).bind(&binding_id).bind(&source_thread_id).bind(&profile_digest).execute(&mut *seed).await.expect("thread");
        sqlx::query("INSERT INTO agent_runtime_host_binding (binding_id,thread_id,offer_id,service_instance_id,instance_revision,driver_generation,profile_digest,state,lease_epoch,binding) VALUES ($1,$2,$3,$4,1,1,$5,'active',1,'{}')")
            .bind(&binding_id).bind(&thread_id).bind(&offer_id).bind(&service_id).bind(&profile_digest).execute(&mut *seed).await.expect("host binding");
        sqlx::query("INSERT INTO agent_run_runtime_thread_anchor (run_id,agent_id,runtime_thread_id,bootstrap_runtime_binding_id) VALUES ($1,$2,$3,$4)")
            .bind(run_id.to_string()).bind(agent_id.to_string()).bind(&thread_id).bind(&binding_id).execute(&mut *seed).await.expect("anchor");
        sqlx::query("INSERT INTO agent_run_runtime_binding_lineage (run_id,agent_id,binding_epoch,runtime_binding_id,binding) VALUES ($1,$2,1,$3,'{}')")
            .bind(run_id.to_string()).bind(agent_id.to_string()).bind(&binding_id).execute(&mut *seed).await.expect("lineage");
        seed.commit().await.expect("seed graph");

        let store = PostgresAgentRunDeleteStore::new(pool.clone());
        let command = DeleteAgentRunCommand { project_id, run_id };
        assert!(matches!(
            store.delete(command).await,
            Err(DeleteAgentRunError::RuntimeActive { .. })
        ));
        sqlx::query("UPDATE agent_runtime_thread SET active_turn_id=NULL WHERE id=$1")
            .bind(&thread_id)
            .execute(&pool)
            .await
            .expect("finish turn");
        sqlx::query("UPDATE lifecycle_agents SET status='cancelling' WHERE id=$1")
            .bind(agent_id.to_string())
            .execute(&pool)
            .await
            .expect("enter cancelling state");
        assert!(matches!(
            store.delete(command).await,
            Err(DeleteAgentRunError::RuntimeActive { .. })
        ));
        sqlx::query("UPDATE lifecycle_agents SET status='completed' WHERE id=$1")
            .bind(agent_id.to_string())
            .execute(&pool)
            .await
            .expect("finish cancellation");
        store.delete(command).await.expect("delete idle AgentRun");

        for (table, id_column, id) in [
            ("lifecycle_runs", "id", run_id.to_string()),
            ("agent_runtime_thread", "id", thread_id.clone()),
            ("agent_runtime_binding", "id", binding_id.clone()),
            (
                "agent_runtime_host_binding",
                "binding_id",
                binding_id.clone(),
            ),
            (
                "agent_run_runtime_thread_anchor",
                "run_id",
                run_id.to_string(),
            ),
        ] {
            let exists: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)"
            ))
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("count deleted fact");
            assert!(!exists, "{table} fact must be deleted");
        }
    }

    async fn test_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("agent run delete uow")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/agent-run-delete-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "agent-run-delete-tests",
            57,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL for AgentRun delete tests");
        let database_name = format!("agent_run_delete_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated AgentRun delete database");
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
            .expect("connect isolated AgentRun delete database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated AgentRun delete database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("AgentRun delete schema readiness");
        (pool, Some(runtime))
    }
}
