# 实施计划

1. Generate/update contracts from API child.
2. Add AgentRun path helpers.
3. Create `useAgentRunWorkspaceState`.
4. Rename/migrate `SessionPage` to `AgentRunWorkspacePage`.
5. Update App routes and layout active path prefixes.
6. Update Project Agent, shortcut list, Run/Subject navigation.
7. Refactor chat view props and executor hydration key.
8. Add AgentConfig JSON mapper from snake_case to executor selector source.
9. Add client command id generation/retry retention.
10. Add focused tests for route generation, workspace hydration, retry command id reuse.
11. Manual validation via `pnpm dev` once backend children are integrated.

## Validation

- `pnpm --filter app-web run typecheck`
- focused vitest for AgentRun workspace route/state
- `rg -n "/session/new|/session/:sessionId|SessionPage" packages/app-web/src`
- manual draft -> AgentRun workspace flow
