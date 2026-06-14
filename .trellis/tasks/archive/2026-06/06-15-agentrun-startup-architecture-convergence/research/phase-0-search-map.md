# Phase 0 Search Map

## Old Model Symbols

| Symbol | Current hit | Meaning |
| --- | --- | --- |
| `ProjectAgentRunInitialMessagePort` | `crates/agentdash-application/src/workflow/project_agent_run_start.rs` | ProjectAgent start owns a separate initial-message launch port. |
| `launch_initial_user_message` | `project_agent_run_start.rs` | Outer start invokes mailbox submit, then adapts it into an initial launch result. |
| `ProjectAgentRunInitialMessageLaunch` | `project_agent_run_start.rs` | Adapter result that requires mailbox delivery accepted refs. |
| `project_agent_initial_launch_from_mailbox_result` | `project_agent_run_start.rs` | Converts mailbox result into old launch-only semantics and rejects queued/blocked/failed outcomes. |
| `accepted_refs_from_initial_launch` | `project_agent_run_start.rs` | Outer start validates inner accepted refs and derives frame refs after delivery. |
| `AgentRunMessageLaunchDeliveryPort` | `crates/agentdash-application/src/workflow/agent_message.rs` and `crates/agentdash-application/src/session/agent_run_mailbox.rs` | Mailbox scheduler delivery adapter for `SessionLaunchService::launch_command`. |
| `SessionLaunchService::launch_command` | `agent_message.rs`, `agent_run_mailbox.rs`, hook dispatch and session tests | Runtime turn launch boundary. Valid for scheduler delivery, but not as ProjectAgent start's accepted boundary. |

## Current Startup Call Graph

```text
POST /projects/{project_id}/agents/{project_agent_id}/agent-runs
  -> ProjectAgentRunStartService::start_run
  -> claim project_agent_start receipt
  -> LifecycleDispatchService::launch_agent
  -> bind ProjectAgent id to LifecycleAgent
  -> ProjectAgentRunInitialMessagePort::launch_initial_user_message
       -> AgentRunMailboxService::accept_user_message
       -> AgentRunMailboxService::schedule
       -> AgentRunMailboxService::consume_as_launch
       -> AgentRunMessageLaunchDeliveryPort::deliver_user_message
       -> SessionLaunchService::launch_command
  -> project_agent_initial_launch_from_mailbox_result
  -> accepted_refs_from_initial_launch
  -> mark project_agent_start accepted only after inner accepted refs
```

Composer submit already uses:

```text
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
  -> AgentRunMailboxService::accept_user_message
  -> command receipt + mailbox envelope + scheduler outcome
```

Phase 1 should make ProjectAgent start expose the same mailbox command/outcome projection for the first user message, while the outer `project_agent_start` receipt only represents durable AgentRun thread/envelope creation.

## Existing Recovery Baseline

- `AgentRunMailboxRepository::recover_expired_consuming(now)` already restores `consuming + accepted refs` to terminal delivery status.
- `consuming + no accepted refs` is expected to become `blocked` with `last_error="delivery_result_unknown"`.
- API projection in `agent_run_mailbox_contracts.rs` sets `can_promote=false` for `delivery_result_unknown`.
- Existing repository tests cover the SQL recovery cases; Phase 1 should add or preserve application-level coverage so scheduler recovery observes the same projection.

## Phase 5 Cleanup Search Terms

Use these searches before the legacy-model cleanup gate:

```powershell
rg -n "ProjectAgentRunInitialMessagePort|launch_initial_user_message|ProjectAgentRunInitialMessageLaunch|project_agent_initial_launch_from_mailbox_result|accepted_refs_from_initial_launch" crates
rg -n "project_agent_start.*launch_command|accepted_refs_from_initial_launch|ProjectAgent start.*accepted refs" crates .trellis
rg -n "AgentRunMessageLaunchDeliveryPort|SessionLaunchService::launch_command|\\.launch_command\\(" crates/agentdash-application/src crates/agentdash-api/src
rg -n "delivery_result_unknown|recover_expired_consuming|MailboxMessageStatus::Consuming|can_promote" crates
rg -n "ProjectAgentRunStartResult|turn_id: string|accepted_refs.*ProjectAgent" packages/app-web/src crates/agentdash-contracts/src crates/agentdash-api/src
```

Residual `launch_command` hits are valid only inside scheduler/session/hook runtime delivery boundaries, not in ProjectAgent start response construction.
