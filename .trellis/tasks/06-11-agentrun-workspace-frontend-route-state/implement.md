# 实施计划

1. [x] Generate/update contracts from API child.
2. [x] Add AgentRun path helpers.
3. [x] Create `useAgentRunWorkspaceState`.
4. [x] Rename/migrate `SessionPage` to `AgentRunWorkspacePage`.
5. [x] Update App routes and layout active path prefixes.
6. [x] Update Project Agent, shortcut list, Run/Subject navigation.
7. [x] Refactor chat view props and executor hydration key.
8. [x] Add AgentConfig JSON mapper from snake_case to executor selector source.
9. [x] Add client command id generation/retry retention.
10. [x] Add focused tests for route generation, workspace hydration, retry command id reuse.
11. [ ] Manual validation via `pnpm dev` once backend children are integrated.

## Integration Adjustment

- Project AgentRun sidebar/list now uses `AgentRunWorkspaceListView` from `/projects/{project_id}/agent-runs`.
- The list projection starts from LifecycleRun/LifecycleAgent and reuses AgentRun workspace shell; RuntimeSession trace meta is only an optional attachment.
- `AgentRunWorkspacePage` no longer lets `session_meta_updated` override the workspace title; the event refreshes the AgentRun workspace projection.

## Validation

- `pnpm --filter app-web run typecheck`
- focused vitest for AgentRun workspace route/state
- `rg -n "/session/new|/session/:sessionId|SessionPage" packages/app-web/src`
- manual draft -> AgentRun workspace flow

## Validation Results

- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`
- `cargo check -p agentdash-api -p agentdash-contracts`
- `cargo clippy -p agentdash-api -p agentdash-contracts -- -D warnings -A clippy::too_many_arguments -A clippy::new_ret_no_self -A clippy::type_complexity -A clippy::collapsible_if -A clippy::collapsible_match -A clippy::redundant_closure -A clippy::map_flatten`
- `pnpm --filter app-web run lint` passed with two pre-existing rounded-full warnings in `SessionChatViewParts.tsx`.
- `pnpm --filter app-web exec vitest run src/features/agent/agent-tab-view.test.ts src/services/lifecycle.test.ts src/features/agent-run-workspace/model/workspaceCommandState.test.ts`
- `rg -n "/session/new|/session/:sessionId|SessionPage|ProjectSessionList|fetchProjectSessionList|active-session-list|SessionShortcutList|session_meta_updated.*set|SessionMeta.title" packages/app-web/src crates/agentdash-api/src crates/agentdash-contracts/src -g "*.ts" -g "*.tsx" -g "*.rs"` returned no matches.
