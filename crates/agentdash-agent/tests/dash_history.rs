use agentdash_agent::dash::{
    AgentHistory, AgentSessionId, AgentTurnId, BranchId, CommandDependency, CommandId,
    CommandOutcome, CommandStatus, CompactionId, CompactionMode, ContextDeliveryFidelity,
    ContextRevision, DashAgentCommit, DashAgentStore, DashCommand, DashCommandKind,
    DashExecutionConsistency, EffectId, EffectOutcome, EffectSettlement, ForkCutoff,
    HistoryContribution, HistoryEntryId, HistoryPayload, InitialContextContribution,
    InitialContextInstallation, InitialContextMode, accepted_compaction_summary_frame,
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
    history
        .append_batch(vec![
            contribution(
                "entry-compaction-start",
                HistoryPayload::CompactionStarted {
                    compaction_id: CompactionId::new("compact-b"),
                    mode: CompactionMode::AutomaticOverflow,
                    source_head,
                    source_digest: source_digest.clone(),
                },
            ),
            contribution(
                "entry-compaction-applied",
                HistoryPayload::CompactionApplied {
                    compaction_id: CompactionId::new("compact-b"),
                    revision: ContextRevision::new("context-r2"),
                    summary: "compacted".into(),
                    retained_from: Some(HistoryEntryId::new("entry-input")),
                    source_digest,
                    context_frame: accepted_compaction_summary_frame(
                        &CompactionId::new("compact-b"),
                        &ContextRevision::new("context-r2"),
                        "compacted",
                    ),
                },
            ),
            contribution(
                "entry-compaction-complete",
                HistoryPayload::CompactionCompleted {
                    compaction_id: CompactionId::new("compact-b"),
                },
            ),
        ])
        .unwrap();

    let replayed = history.state().unwrap();
    let compaction = replayed
        .compactions
        .get(&CompactionId::new("compact-b"))
        .unwrap();
    assert_eq!(
        compaction.revision.as_ref().unwrap(),
        &ContextRevision::new("context-r2")
    );
    assert_eq!(compaction.summary.as_deref(), Some("compacted"));
    assert!(replayed.active_compaction.is_none());
}

#[test]
fn dash_agent_commit_is_atomic_across_effect_history_change_and_continuation() {
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
            effect_settlements: vec![EffectSettlement {
                effect_id: EffectId::new("effect-a"),
                outcome: EffectOutcome::Applied,
            }],
            history: vec![],
            enqueue_commands: vec![compaction_command, continuation],
        })
        .unwrap();

    let promoted_b = store.lifecycle().clone().promote_next().unwrap().unwrap();
    assert_eq!(promoted_b.command_id, CommandId::new("command-b"));

    // Invalid head rejects the whole commit before any effect or history mutation.
    let before = store.clone();
    let error = store
        .commit(DashAgentCommit {
            expected_head: Some(HistoryEntryId::new("stale-head")),
            command_settlement: None,
            effect_settlements: vec![EffectSettlement {
                effect_id: EffectId::new("effect-never"),
                outcome: EffectOutcome::Applied,
            }],
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
                        HistoryPayload::TurnCompleted { turn_id },
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
            effect_settlements: vec![],
            history: vec![],
            enqueue_commands: vec![input],
        })
        .unwrap();
    assert!(store.claim_next_command().unwrap().is_none());

    store
        .complete_compaction(
            CommandId::new("manual-b"),
            EffectId::new("manual-effect"),
            CompactionId::new("manual-b"),
            ContextRevision::new("manual-r2"),
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
            effect_settlements: vec![],
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
            EffectId::new("auto-effect"),
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
        store.effect_outcome(&EffectId::new("auto-effect")),
        Some(EffectOutcome::Failed)
    );
    let inspection =
        store.inspect_execution(&CommandId::new("auto-b"), &EffectId::new("auto-effect"));
    assert_eq!(inspection.command_status, Some(CommandStatus::Failed));
    assert_eq!(inspection.effect_outcome, Some(EffectOutcome::Failed));
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
            },
        ))
        .unwrap_err();
    assert!(error.to_string().contains("digest"));
    assert_eq!(history, before);
}

#[test]
fn dash_commit_rolls_back_earlier_staged_settlement_when_later_effect_conflicts() {
    let mut store = DashAgentStore::new(history_with_turn()).unwrap();
    let command = DashCommand {
        command_id: CommandId::new("atomic-command"),
        kind: DashCommandKind::SubmitInput {
            input_id: "atomic".into(),
            content: "atomic".into(),
        },
        dependency: None,
    };
    store
        .commit(DashAgentCommit {
            expected_head: store.history().head().cloned(),
            command_settlement: None,
            effect_settlements: vec![EffectSettlement {
                effect_id: EffectId::new("atomic-effect"),
                outcome: EffectOutcome::Applied,
            }],
            history: vec![],
            enqueue_commands: vec![command],
        })
        .unwrap();
    store.claim_next_command().unwrap();
    let before = store.clone();

    let error = store
        .commit(DashAgentCommit {
            expected_head: store.history().head().cloned(),
            command_settlement: Some(agentdash_agent::dash::CommandSettlement {
                command_id: CommandId::new("atomic-command"),
                outcome: CommandOutcome::Succeeded,
            }),
            effect_settlements: vec![EffectSettlement {
                effect_id: EffectId::new("atomic-effect"),
                outcome: EffectOutcome::Failed,
            }],
            history: vec![contribution(
                "atomic-never-appended",
                HistoryPayload::InputAccepted {
                    input_id: "never".into(),
                    content: "never".into(),
                },
            )],
            enqueue_commands: vec![],
        })
        .unwrap_err();
    assert!(error.to_string().contains("conflicting terminal"));
    assert_eq!(store, before);
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
fn ordered_changes_capture_incremental_history_and_active_turn_facts() {
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
            effect_settlements: vec![],
            history: vec![
                contribution(
                    "change-start",
                    HistoryPayload::TurnStarted {
                        turn_id: turn_id.clone(),
                    },
                ),
                contribution(
                    "change-completed",
                    HistoryPayload::TurnCompleted {
                        turn_id: turn_id.clone(),
                    },
                ),
            ],
            enqueue_commands: vec![],
        })
        .unwrap();

    let changes = store.changes();
    assert_eq!(changes.len(), 4);
    assert!(matches!(
        &changes[0].payload,
        agentdash_agent::dash::DashAgentChangePayload::HistoryEntry {
            entry: agentdash_agent::dash::AgentHistoryEntry {
                payload: HistoryPayload::TurnStarted { turn_id: started },
                ..
            }
        } if started == &turn_id
    ));
    assert!(matches!(
        &changes[1].payload,
        agentdash_agent::dash::DashAgentChangePayload::ActiveTurnChanged {
            active_turn_id: Some(active),
        } if active == &turn_id
    ));
    assert!(matches!(
        &changes[2].payload,
        agentdash_agent::dash::DashAgentChangePayload::HistoryEntry {
            entry: agentdash_agent::dash::AgentHistoryEntry {
                payload: HistoryPayload::TurnCompleted { turn_id: completed },
                ..
            }
        } if completed == &turn_id
    ));
    assert!(matches!(
        &changes[3].payload,
        agentdash_agent::dash::DashAgentChangePayload::ActiveTurnChanged {
            active_turn_id: None,
        }
    ));
    assert_eq!(store.changes()[1].cursor.encode(), "1:1");
    assert_ne!(changes[0].source_digest, changes[2].source_digest);
}
