use agentdash_agent::dash::{
    AgentHistory, AgentSessionId, AgentTurnId, BranchId, CommandDependency, CommandId,
    CommandOutcome, CommandStatus, CompactionId, CompactionMode, ContextDeliveryFidelity,
    DashAgentCommit, DashAgentStore, DashCommand, DashCommandKind, DashExecutionConsistency,
    ForkCutoff, HistoryContribution, HistoryEntryId, HistoryPayload, InitialContextContribution,
    InitialContextInstallation, InitialContextMode, accepted_compaction_summary_frame,
    compaction_context_revision,
};

fn contribution(id: &str, payload: HistoryPayload) -> HistoryContribution {
    HistoryContribution {
        entry_id: HistoryEntryId::new(id),
        payload,
    }
}

fn initial_package() -> InitialContextInstallation {
    InitialContextInstallation {
        package_id: "package-1".into(),
        package_digest: "digest-1".into(),
        mode: InitialContextMode::Compact,
        fidelity: ContextDeliveryFidelity::TypedNative,
        contributions: vec![InitialContextContribution {
            kind: "compact_summary".into(),
            payload: "summary".into(),
            authority: "agent_history".into(),
            source_revision: "source-r1".into(),
            digest: "contribution-digest".into(),
        }],
        context_frames: Vec::new(),
    }
}

fn history_with_turn() -> AgentHistory {
    let mut history = AgentHistory::empty(
        AgentSessionId::new("session-parent"),
        BranchId::new("branch-parent"),
    );
    history
        .append_batch(vec![
            contribution(
                "entry-context",
                HistoryPayload::InitialContextInstalled {
                    installation: initial_package(),
                },
            ),
            contribution(
                "entry-input",
                HistoryPayload::InputAccepted {
                    input_id: "input-1".into(),
                    content: "hello".into(),
                },
            ),
            contribution(
                "entry-turn-start",
                HistoryPayload::TurnStarted {
                    turn_id: AgentTurnId::new("turn-a"),
                    started_at_ms: 1_000,
                },
            ),
            contribution(
                "entry-output",
                HistoryPayload::AgentOutput {
                    turn_id: AgentTurnId::new("turn-a"),
                    item_id: None,
                    content: "answer".into(),
                },
            ),
            contribution(
                "entry-turn-complete",
                HistoryPayload::TurnCompleted {
                    turn_id: AgentTurnId::new("turn-a"),
                    completed_at_ms: 2_000,
                },
            ),
        ])
        .unwrap();
    history
}

#[test]
fn session_projection_is_only_the_history_fold() {
    let history = history_with_turn();
    let replayed = history.state().unwrap();

    assert_eq!(replayed.entry_count, 5);
    assert_eq!(replayed.initial_context.unwrap().package_digest, "digest-1");
    assert_eq!(replayed.accepted_inputs, vec!["input-1"]);
    assert_eq!(
        replayed
            .turns
            .get(&AgentTurnId::new("turn-a"))
            .unwrap()
            .output
            .as_deref(),
        Some("answer")
    );
    assert!(replayed.active_turn.is_none());
}

#[test]
fn fresh_context_and_first_input_are_distinct_history_contributions() {
    let history = history_with_turn();
    assert!(matches!(
        history.entries()[0].payload,
        HistoryPayload::InitialContextInstalled { .. }
    ));
    assert!(matches!(
        history.entries()[1].payload,
        HistoryPayload::InputAccepted { .. }
    ));
    assert_ne!(history.entries()[0].entry_id, history.entries()[1].entry_id);
}

#[test]
fn exact_fork_has_independent_head_and_replayable_lineage() {
    let parent = history_with_turn();
    let parent_digest = parent.digest();
    let mut child = parent
        .fork(
            AgentSessionId::new("session-child"),
            BranchId::new("branch-child"),
            ForkCutoff::CompletedTurn {
                turn_id: AgentTurnId::new("turn-a"),
            },
        )
        .unwrap();

    assert_eq!(child.lineage.as_ref().unwrap().source_digest, parent_digest);
    child
        .append(contribution(
            "entry-child-input",
            HistoryPayload::InputAccepted {
                input_id: "input-child".into(),
                content: "branch".into(),
            },
        ))
        .unwrap();

    assert_eq!(parent.entries().len(), 5);
    assert_eq!(child.entries().len(), 6);
    assert_eq!(
        child.state().unwrap().accepted_inputs,
        vec!["input-1", "input-child"]
    );
}

#[test]
fn compaction_is_a_provenance_preserving_history_transformation() {
    let mut history = history_with_turn();
    let source_head = history.head().cloned();
    let source_digest = history.digest();
    let revision = compaction_context_revision(
        &CompactionId::new("compact-b"),
        &source_digest,
        "compacted",
        Some(&HistoryEntryId::new("entry-input")),
    );
    let summary_frame = accepted_compaction_summary_frame(
        &CompactionId::new("compact-b"),
        &revision,
        "compacted",
        CompactionMode::AutomaticOverflow,
        0,
        0,
        None,
        None,
        Some(2),
        2_000,
    );
    history
        .append_batch(vec![
            contribution(
                "entry-compaction-start",
                HistoryPayload::CompactionStarted {
                    compaction_id: CompactionId::new("compact-b"),
                    mode: CompactionMode::AutomaticOverflow,
                    source_head: source_head.clone(),
                    source_digest: source_digest.clone(),
                    started_at_ms: 1_000,
                },
            ),
            contribution(
                "entry-compaction-side-effect-started",
                HistoryPayload::CompactionSideEffectStarted {
                    compaction_id: CompactionId::new("compact-b"),
                    started_at_ms: 1_500,
                },
            ),
            contribution(
                "entry-compaction-applied",
                HistoryPayload::CompactionApplied {
                    compaction_id: CompactionId::new("compact-b"),
                    context_revision: revision.clone(),
                    summary_frame: Box::new(summary_frame),
                    retained_from: Some(HistoryEntryId::new("entry-input")),
                },
            ),
            contribution(
                "entry-compaction-complete",
                HistoryPayload::CompactionCompleted {
                    compaction_id: CompactionId::new("compact-b"),
                    completed_at_ms: 3_000,
                },
            ),
        ])
        .unwrap();

    let replayed = history.state().unwrap();
    let compaction = replayed
        .compactions
        .get(&CompactionId::new("compact-b"))
        .unwrap();
    assert_eq!(compaction.context_revision.as_ref(), Some(&revision));
    assert_eq!(
        compaction.retained_from.as_ref(),
        Some(&HistoryEntryId::new("entry-input"))
    );
    assert!(replayed.active_compaction.is_none());
    let compaction_turn = replayed
        .turns
        .get(&AgentTurnId::new("compact-b"))
        .expect("formal compaction turn");
    assert_eq!(compaction_turn.started_at_ms, 1_000);
    assert_eq!(compaction_turn.completed_at_ms, Some(3_000));
}

#[test]
fn dash_agent_commit_is_atomic_across_history_and_continuation() {
    let history = history_with_turn();
    let mut store = DashAgentStore::new(history).unwrap();
    let compaction_command = DashCommand {
        command_id: CommandId::new("command-b"),
        kind: DashCommandKind::RequestCompaction {
            compaction_id: CompactionId::new("compact-b"),
            mode: CompactionMode::AutomaticOverflow,
        },
        dependency: None,
    };
    let continuation = DashCommand {
        command_id: CommandId::new("command-c"),
        kind: DashCommandKind::ContinueAfterCompaction {
            input_id: "continuation-input".into(),
            content: "retry original input".into(),
        },
        dependency: Some(CommandDependency {
            command_id: CommandId::new("command-b"),
        }),
    };
    store
        .commit(DashAgentCommit {
            expected_head: store.history().head().cloned(),
            command_settlement: None,
            history: vec![],
            enqueue_commands: vec![compaction_command, continuation],
        })
        .unwrap();

    let promoted_b = store.lifecycle().clone().promote_next().unwrap().unwrap();
    assert_eq!(promoted_b.command_id, CommandId::new("command-b"));

    // Invalid head rejects the whole commit before any history or command mutation.
    let before = store.clone();
    let error = store
        .commit(DashAgentCommit {
            expected_head: Some(HistoryEntryId::new("stale-head")),
            command_settlement: None,
            history: vec![contribution(
                "entry-never",
                HistoryPayload::InputAccepted {
                    input_id: "never".into(),
                    content: "never".into(),
                },
            )],
            enqueue_commands: vec![],
        })
        .unwrap_err();
    assert!(error.to_string().contains("head conflict"));
    assert_eq!(store, before);
}

#[test]
fn automatic_overflow_keeps_a_b_c_separate_and_promotes_c_explicitly() {
    let mut lifecycle = agentdash_agent::dash::DashLifecycle::default();
    let command_b = DashCommand {
        command_id: CommandId::new("B"),
        kind: DashCommandKind::RequestCompaction {
            compaction_id: CompactionId::new("compaction-B"),
            mode: CompactionMode::AutomaticOverflow,
        },
        dependency: None,
    };
    let command_c = DashCommand {
        command_id: CommandId::new("C"),
        kind: DashCommandKind::ContinueAfterCompaction {
            input_id: "input-C".into(),
            content: "continue".into(),
        },
        dependency: Some(CommandDependency {
            command_id: CommandId::new("B"),
        }),
    };
    lifecycle.enqueue(command_b).unwrap();
    lifecycle.enqueue(command_c).unwrap();

    assert_eq!(
        lifecycle.promote_next().unwrap().unwrap().command_id,
        CommandId::new("B")
    );
    lifecycle
        .settle_active(&CommandId::new("B"), CommandOutcome::Succeeded)
        .unwrap();
    assert!(lifecycle.active().is_none());
    assert_eq!(
        lifecycle.status(&CommandId::new("C")),
        Some(CommandStatus::Queued)
    );

    // B terminal does not implicitly create/start C; promotion is a separate action.
    assert_eq!(
        lifecycle.promote_next().unwrap().unwrap().command_id,
        CommandId::new("C")
    );
}

#[test]
fn clean_compaction_failure_terminalizes_c_while_lost_blocks_it() {
    for (outcome, expected, consistency) in [
        (
            CommandOutcome::Failed,
            CommandStatus::Failed,
            DashExecutionConsistency::Current,
        ),
        (
            CommandOutcome::Lost,
            CommandStatus::Blocked,
            DashExecutionConsistency::Lost,
        ),
    ] {
        let mut lifecycle = agentdash_agent::dash::DashLifecycle::default();
        lifecycle
            .enqueue(DashCommand {
                command_id: CommandId::new("B"),
                kind: DashCommandKind::RequestCompaction {
                    compaction_id: CompactionId::new("B"),
                    mode: CompactionMode::AutomaticOverflow,
                },
                dependency: None,
            })
            .unwrap();
        lifecycle
            .enqueue(DashCommand {
                command_id: CommandId::new("C"),
                kind: DashCommandKind::ContinueAfterCompaction {
                    input_id: "C".into(),
                    content: "continue".into(),
                },
                dependency: Some(CommandDependency {
                    command_id: CommandId::new("B"),
                }),
            })
            .unwrap();
        lifecycle.promote_next().unwrap();
        lifecycle
            .settle_active(&CommandId::new("B"), outcome)
            .unwrap();

        assert_eq!(lifecycle.status(&CommandId::new("C")), Some(expected));
        assert_eq!(lifecycle.consistency, consistency);
        assert!(lifecycle.promote_next().unwrap().is_none());
    }
}

#[test]
fn replay_property_holds_across_many_history_shapes_and_serialization() {
    for seed in 0..32 {
        let mut history = AgentHistory::empty(
            AgentSessionId::new(format!("session-{seed}")),
            BranchId::new(format!("branch-{seed}")),
        );
        if seed % 2 == 0 {
            history
                .append(contribution(
                    &format!("{seed}-context"),
                    HistoryPayload::InitialContextInstalled {
                        installation: initial_package(),
                    },
                ))
                .unwrap();
        }
        for turn in 0..(seed % 5 + 1) {
            let turn_id = AgentTurnId::new(format!("{seed}-turn-{turn}"));
            history
                .append_batch(vec![
                    contribution(
                        &format!("{seed}-{turn}-input"),
                        HistoryPayload::InputAccepted {
                            input_id: format!("input-{turn}"),
                            content: format!("content-{turn}"),
                        },
                    ),
                    contribution(
                        &format!("{seed}-{turn}-start"),
                        HistoryPayload::TurnStarted {
                            turn_id: turn_id.clone(),
                            started_at_ms: turn as u64 * 1_000,
                        },
                    ),
                    contribution(
                        &format!("{seed}-{turn}-output"),
                        HistoryPayload::AgentOutput {
                            turn_id: turn_id.clone(),
                            item_id: None,
                            content: format!("output-{turn}"),
                        },
                    ),
                    contribution(
                        &format!("{seed}-{turn}-complete"),
                        HistoryPayload::TurnCompleted {
                            turn_id,
                            completed_at_ms: turn as u64 * 1_000 + 500,
                        },
                    ),
                ])
                .unwrap();
        }
        let expected = history.state().unwrap();
        let encoded = serde_json::to_vec(&history).unwrap();
        let restored: AgentHistory = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored.state().unwrap(), expected);
        assert_eq!(restored.digest(), history.digest());
    }
}

#[test]
fn manual_compaction_defers_new_input_until_terminal_then_promotes_explicitly() {
    let mut store = DashAgentStore::new(history_with_turn()).unwrap();
    let compaction = DashCommand {
        command_id: CommandId::new("manual-b"),
        kind: DashCommandKind::RequestCompaction {
            compaction_id: CompactionId::new("manual-b"),
            mode: CompactionMode::Manual,
        },
        dependency: None,
    };
    store
        .begin_compaction(compaction, HistoryEntryId::new("manual-start"))
        .unwrap();
    let input = DashCommand {
        command_id: CommandId::new("input-after-manual"),
        kind: DashCommandKind::SubmitInput {
            input_id: "input-after-manual".into(),
            content: "wait".into(),
        },
        dependency: None,
    };
    store
        .commit(DashAgentCommit {
            expected_head: store.history().head().cloned(),
            command_settlement: None,
            history: vec![],
            enqueue_commands: vec![input],
        })
        .unwrap();
    assert!(store.claim_next_command().unwrap().is_none());

    store
        .mark_compaction_side_effect_started(
            CompactionId::new("manual-b"),
            HistoryEntryId::new("manual-side-effect-started"),
        )
        .unwrap();
    store
        .complete_compaction(
            CommandId::new("manual-b"),
            CompactionId::new("manual-b"),
            "summary".into(),
            None,
            HistoryEntryId::new("manual-applied"),
            HistoryEntryId::new("manual-completed"),
        )
        .unwrap();
    assert!(store.lifecycle().active().is_none());
    assert_eq!(
        store.claim_next_command().unwrap().unwrap().command_id,
        CommandId::new("input-after-manual")
    );
}

#[test]
fn automatic_compaction_failure_settles_dependent_continuation_in_same_commit() {
    let mut store = DashAgentStore::new(history_with_turn()).unwrap();
    store
        .begin_compaction(
            DashCommand {
                command_id: CommandId::new("auto-b"),
                kind: DashCommandKind::RequestCompaction {
                    compaction_id: CompactionId::new("auto-b"),
                    mode: CompactionMode::AutomaticOverflow,
                },
                dependency: None,
            },
            HistoryEntryId::new("auto-start"),
        )
        .unwrap();
    store
        .commit(DashAgentCommit {
            expected_head: store.history().head().cloned(),
            command_settlement: None,
            history: vec![],
            enqueue_commands: vec![DashCommand {
                command_id: CommandId::new("auto-c"),
                kind: DashCommandKind::ContinueAfterCompaction {
                    input_id: "auto-c".into(),
                    content: "continue".into(),
                },
                dependency: Some(CommandDependency {
                    command_id: CommandId::new("auto-b"),
                }),
            }],
        })
        .unwrap();
    store
        .fail_compaction(
            CommandId::new("auto-b"),
            CompactionId::new("auto-b"),
            HistoryEntryId::new("auto-failed"),
            "clean failure".into(),
            false,
        )
        .unwrap();

    assert_eq!(
        store.command_status(&CommandId::new("auto-c")),
        Some(CommandStatus::Failed)
    );
    assert_eq!(
        store.command_status(&CommandId::new("auto-b")),
        Some(CommandStatus::Failed)
    );
    assert!(store.history().state().unwrap().active_compaction.is_none());
}

#[test]
fn invalid_compaction_provenance_does_not_mutate_history() {
    let mut history = history_with_turn();
    let before = history.clone();
    let error = history
        .append(contribution(
            "bad-compaction",
            HistoryPayload::CompactionStarted {
                compaction_id: CompactionId::new("bad"),
                mode: CompactionMode::Manual,
                source_head: history.head().cloned(),
                source_digest: "forged".into(),
                started_at_ms: 1_000,
            },
        ))
        .unwrap_err();
    assert!(error.to_string().contains("digest"));
    assert_eq!(history, before);
}

#[test]
fn session_projection_contains_no_command_effect_or_platform_coordination_state() {
    let state = history_with_turn().state().unwrap();
    let value = serde_json::to_value(state).unwrap();
    let object = value.as_object().unwrap();
    for forbidden in [
        "command",
        "effect",
        "mailbox",
        "binding",
        "generation",
        "lease",
        "operation",
    ] {
        assert!(
            !object.contains_key(forbidden),
            "{forbidden} leaked into Session"
        );
    }
}

#[test]
fn ordered_changes_are_derived_once_from_incremental_history() {
    let mut store = DashAgentStore::new(AgentHistory::empty(
        AgentSessionId::new("change-session"),
        BranchId::new("change-branch"),
    ))
    .unwrap();
    let turn_id = AgentTurnId::new("change-turn");
    store
        .commit(DashAgentCommit {
            expected_head: None,
            command_settlement: None,
            history: vec![
                contribution(
                    "change-start",
                    HistoryPayload::TurnStarted {
                        turn_id: turn_id.clone(),
                        started_at_ms: 1_000,
                    },
                ),
                contribution(
                    "change-completed",
                    HistoryPayload::TurnCompleted {
                        turn_id: turn_id.clone(),
                        completed_at_ms: 2_000,
                    },
                ),
            ],
            enqueue_commands: vec![],
        })
        .unwrap();

    let changes = store.changes().unwrap();
    assert_eq!(changes.len(), 2);
    assert!(matches!(
        &changes[0].entry.payload,
        HistoryPayload::TurnStarted {
            turn_id: started,
            ..
        } if started == &turn_id
    ));
    assert!(matches!(
        &changes[1].entry.payload,
        HistoryPayload::TurnCompleted {
            turn_id: completed,
            ..
        } if completed == &turn_id
    ));
    assert_eq!(changes[0].cursor.encode(), "1");
    assert_eq!(changes[1].cursor.encode(), "2");
    assert_ne!(changes[0].source_digest, changes[1].source_digest);
}
