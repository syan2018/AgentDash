# AgentRun Workspace Control Plane 深模块评估 - Implement

## Phase 0 - Evaluation Only

- [ ] Map `AgentRunWorkspacePage` responsibilities into layout, control-plane, workspace panel, identity bar, route navigation.
- [ ] Map `SessionChatView` props into stream/feed, composer command model, mailbox model, executor model, layout slots.
- [ ] Replace `SessionChatView` props in the first slice with narrower chat/composer/mailbox view models and intent handlers.
- [ ] Confirm no user-visible command behavior changes in first slice; backend DTO and internal props may change if the new interface is cleaner.
- [ ] Produce an ownership deletion map: page/command hook/ChatView responsibility -> control-plane model / thin adapter / deleted.

## Phase 1 - First Slice Candidate

- [ ] Add `useAgentRunWorkspaceControlPlane` or equivalent model module.
- [ ] Move command state construction, mailbox command lookup and refresh effects from page into control-plane module.
- [ ] Replace `SessionChatView` public props with UI model + intent handlers; keep generated command DTO handling inside control-plane.
- [ ] Add tests around control-plane model where possible.
- [ ] Preserve existing page and ChatView tests until coverage is moved.

## Phase 2 - ChatView Interface Candidate

- [ ] Define `ConversationInputModel` / `SessionComposerControl`.
- [ ] Move generated `ConversationCommandSetView` handling behind adapter.
- [ ] Shrink `SessionChatView` interface to UI model + submit intent handlers.
- [ ] Migrate tests from backend DTO fixtures to UI intent model fixtures.
- [ ] Delete or demote old scattered ownership; do not leave page, hook and ChatView sharing the same command policy.

## Validation

- `pnpm --filter app-web test`
- `pnpm --filter app-web typecheck`
- Targeted tests around `AgentRunWorkspacePage`, `useAgentRunWorkspaceCommands`, `SessionChatView`

## Stop Conditions

- If implementation pressure pushes toward compatibility branches or duplicate old/new command models, stop and return to design review.
- If adapter starts owning stream/feed rendering, split that into a later phase.
- If old page/hook/ChatView code still owns command projection, mailbox lookup or refresh policy after the supposed migration, the task is not complete.
