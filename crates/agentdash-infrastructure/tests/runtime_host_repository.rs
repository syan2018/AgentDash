use std::collections::{BTreeMap, BTreeSet};

use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::*;
use agentdash_infrastructure::{
    PostgresAgentRuntimeHostRepository, postgres_runtime::PostgresRuntime,
};
use chrono::{Duration, Utc};
use serde_json::json;

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid test id")
}

fn profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::Turn,
        input: InputProfile {
            modalities: BTreeSet::from([InputModality::Text]),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        tools: ToolProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
            cancellation: false,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::new(),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: false,
        },
        lifecycle: BTreeSet::from([LifecycleCapability::ThreadStart]),
        hooks: HookProfile {
            points: vec![],
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::Opaque,
            activation_idempotent: false,
        },
        telemetry_config: BTreeSet::new(),
    }
}

async fn database() -> (PostgresRuntime, sqlx::PgPool) {
    let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-host-postgres-tests");
    let runtime =
        PostgresRuntime::resolve_embedded_at_data_root("runtime-host-tests", 56, data_root)
            .await
            .expect("start embedded PostgreSQL for Driver Host tests");
    let database_name = format!("runtime_host_test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database_name}"))
        .execute(&runtime.pool)
        .await
        .expect("create Host test database");
    let options = runtime
        .pool
        .connect_options()
        .as_ref()
        .clone()
        .database(&database_name);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .expect("connect Host test database");
    agentdash_infrastructure::migration::run_postgres_migrations(&pool)
        .await
        .expect("migrate Host test database");
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&pool)
        .await
        .expect("Host schema readiness");
    (runtime, pool)
}

#[tokio::test]
async fn postgres_host_repository_keeps_activation_binding_source_and_lease_atomic() {
    let (_runtime, pool) = database().await;
    let repository = PostgresAgentRuntimeHostRepository::new(pool.clone());
    let instance_id: RuntimeServiceInstanceId = id("instance-pg");
    let instance = repository
        .put_instance(
            AgentServiceInstance {
                id: instance_id.clone(),
                definition_id: AgentServiceDefinitionId::new("corp.pg").expect("definition"),
                definition_build_digest: "sha256:build".to_string(),
                config: json!({"endpoint": "local"}),
                credentials: BTreeMap::new(),
                placement: AgentRuntimePlacement::InProcess,
                desired_state: ServiceInstanceDesiredState::Active,
                observed_state: ServiceInstanceObservedState::Inactive,
                revision: 0,
            },
            None,
        )
        .await
        .expect("put instance");
    let generation = repository
        .next_generation(&instance_id, instance.revision)
        .await
        .expect("generation");
    let runtime_profile = profile();
    let digest = profile_digest(&runtime_profile).expect("profile digest");
    let mut active_instance = instance.clone();
    active_instance.observed_state = ServiceInstanceObservedState::Active;
    let offer = RuntimeOffer {
        id: AgentServiceOfferId::new("offer-pg").expect("offer"),
        service_instance_id: instance_id.clone(),
        instance_revision: instance.revision,
        generation,
        provenance: AgentServiceProvenance {
            definition_id: instance.definition_id.clone(),
            publisher_integration: "corp.integration".to_string(),
            service_version: "1.0.0".to_string(),
            build_digest: AgentServiceBuildDigest::new("sha256:build").expect("build"),
        },
        placement: AgentRuntimePlacement::InProcess,
        protocol_revision: 1,
        effective_profile: EffectiveRuntimeProfile {
            profile: runtime_profile,
            provenance: ProfileProvenance {
                service_digest: digest.clone(),
                transport_digest: digest.clone(),
                host_policy_digest: digest.clone(),
            },
        },
        profile_digest: digest.clone(),
        conformance: ConformanceEvidence {
            suite_revision: "v1".to_string(),
            driver_build_digest: "sha256:driver".to_string(),
            verified_profile_digest: digest.clone(),
            verified_at: Utc::now(),
        },
        available: true,
    };
    repository
        .commit_activation(active_instance.clone(), offer.clone())
        .await
        .expect("commit activation");
    assert_eq!(
        repository
            .load_activation_instance(&instance_id, generation)
            .await
            .expect("activation snapshot"),
        Some(active_instance.clone())
    );

    let binding_id: RuntimeBindingId = id("binding-pg");
    let thread_id: RuntimeThreadId = id("thread-pg");
    let surface = BoundAgentSurfaceReference {
        revision: SurfaceRevision(1),
        digest: id("sha256:surface"),
        hook_plan_revision: None,
        hook_plan_digest: None,
        hook_artifact_digest: None,
        hook_configuration_boundary: ConfigurationBoundary::Binding,
        required_hooks: vec![],
    };
    repository
        .reserve_binding(RuntimeBinding {
            id: binding_id.clone(),
            thread_id: thread_id.clone(),
            offer_id: offer.id.clone(),
            service_instance_id: instance_id,
            instance_revision: instance.revision,
            driver_generation: generation,
            profile_digest: digest.clone(),
            bound_surface: surface.clone(),
            applied_surface: None,
            driver_binding_id: None,
            source_thread_id: None,
            state: RuntimeBindingState::Pending,
            lease_epoch: 0,
        })
        .await
        .expect("reserve binding");
    let applied = AppliedSurface {
        revision: surface.revision,
        digest: surface.digest.clone(),
        hook_plan_revision: None,
        hook_plan_digest: None,
        hooks: vec![],
    };
    let source = RuntimeSourceCoordinate {
        binding_id: binding_id.clone(),
        generation,
        thread_id: thread_id.clone(),
        source_thread_id: id("source-pg"),
    };
    let binding = repository
        .activate_binding(
            &binding_id,
            generation,
            applied,
            id("driver-binding-pg"),
            source.clone(),
        )
        .await
        .expect("activate binding");
    assert_eq!(binding.state, RuntimeBindingState::Active);
    assert_eq!(
        repository
            .find_binding_by_thread(&thread_id)
            .await
            .expect("thread binding"),
        Some(binding)
    );
    assert_eq!(
        repository
            .find_source(&binding_id, generation)
            .await
            .expect("source"),
        Some(source)
    );
    repository
        .record_driver_coordinate(
            &binding_id,
            generation,
            RuntimeDriverCoordinate::Turn {
                runtime_turn_id: id("turn-pg"),
                source_turn_id: id("source-turn-pg"),
            },
        )
        .await
        .expect("record driver turn coordinate");
    assert!(
        repository
            .record_driver_coordinate(
                &binding_id,
                generation,
                RuntimeDriverCoordinate::Turn {
                    runtime_turn_id: id("other-turn-pg"),
                    source_turn_id: id("source-turn-pg"),
                },
            )
            .await
            .is_err()
    );

    let now = Utc::now();
    let lease = repository
        .acquire_lease(
            &binding_id,
            generation,
            "host-a",
            now,
            now + Duration::seconds(30),
        )
        .await
        .expect("lease");
    let replayed_lease = repository
        .acquire_lease(
            &binding_id,
            generation,
            "host-a",
            Utc::now(),
            Utc::now() + Duration::seconds(30),
        )
        .await
        .expect("same owner lease replay");
    assert_eq!(replayed_lease, lease);
    repository
        .validate_lease(&binding_id, generation, "host-a", &lease.token, Utc::now())
        .await
        .expect("valid lease");
    assert!(
        repository
            .validate_lease(
                &binding_id,
                RuntimeDriverGeneration(generation.0 + 1),
                "host-a",
                &lease.token,
                Utc::now(),
            )
            .await
            .is_err()
    );

    sqlx::query(
        "UPDATE agent_runtime_driver_lease SET expires_at=clock_timestamp()-interval '1 second' WHERE binding_id=$1",
    )
    .bind(binding_id.as_str())
    .execute(&pool)
    .await
    .expect("expire first lease using database clock");
    let takeover = repository
        .acquire_lease(
            &binding_id,
            generation,
            "host-b",
            Utc::now(),
            Utc::now() + Duration::seconds(30),
        )
        .await
        .expect("take over expired lease");
    assert!(takeover.epoch > lease.epoch);
    assert!(
        repository
            .validate_lease(&binding_id, generation, "host-a", &lease.token, Utc::now(),)
            .await
            .is_err()
    );
    repository
        .validate_lease(
            &binding_id,
            generation,
            "host-b",
            &takeover.token,
            Utc::now(),
        )
        .await
        .expect("takeover lease is valid");

    let rollback_binding_id: RuntimeBindingId = id("binding-rollback-pg");
    assert!(
        repository
            .reserve_binding(RuntimeBinding {
                id: rollback_binding_id.clone(),
                thread_id: thread_id.clone(),
                offer_id: offer.id.clone(),
                service_instance_id: offer.service_instance_id.clone(),
                instance_revision: offer.instance_revision,
                driver_generation: offer.generation,
                profile_digest: offer.profile_digest.clone(),
                bound_surface: surface.clone(),
                applied_surface: None,
                driver_binding_id: None,
                source_thread_id: None,
                state: RuntimeBindingState::Pending,
                lease_epoch: 0,
            })
            .await
            .is_err()
    );
    let leaked_anchor: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_runtime_binding WHERE id=$1")
            .bind(rollback_binding_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("rollback anchor count");
    assert_eq!(leaked_anchor, 0);

    let mut updated_instance = active_instance.clone();
    updated_instance.config = json!({"endpoint": "changed"});
    updated_instance.observed_state = ServiceInstanceObservedState::Inactive;
    let updated_instance = repository
        .put_instance(updated_instance, Some(instance.revision))
        .await
        .expect("instance revision update preserves activation history");
    assert_eq!(updated_instance.revision, 2);
    assert_eq!(
        repository
            .load_activation_instance(&updated_instance.id, generation)
            .await
            .expect("old activation snapshot"),
        Some(active_instance)
    );
    let revision_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_runtime_service_instance_revision WHERE service_instance_id=$1",
    )
    .bind(updated_instance.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("revision history count");
    assert_eq!(revision_count, 2);

    let stale_binding_id: RuntimeBindingId = id("binding-stale-offer-pg");
    assert!(
        repository
            .reserve_binding(RuntimeBinding {
                id: stale_binding_id,
                thread_id: id("thread-stale-offer-pg"),
                offer_id: offer.id,
                service_instance_id: updated_instance.id.clone(),
                instance_revision: instance.revision,
                driver_generation: generation,
                profile_digest: offer.profile_digest,
                bound_surface: surface,
                applied_surface: None,
                driver_binding_id: None,
                source_thread_id: None,
                state: RuntimeBindingState::Pending,
                lease_epoch: 0,
            })
            .await
            .is_err()
    );

    let mut left_update = updated_instance.clone();
    left_update.config = json!({"endpoint": "left"});
    let mut right_update = updated_instance.clone();
    right_update.config = json!({"endpoint": "right"});
    let (left, right) = tokio::join!(
        repository.put_instance(left_update, Some(updated_instance.revision)),
        repository.put_instance(right_update, Some(updated_instance.revision)),
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let history_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_runtime_service_instance_revision WHERE service_instance_id=$1",
    )
    .bind(updated_instance.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("post-CAS history count");
    assert_eq!(history_count, 3);
}
