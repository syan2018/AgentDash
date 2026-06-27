# Implement

1. Move `workspaceTabStore` imports to `features/workspace-runtime` types and define pure layout descriptor/options contracts in the store.
2. Replace store registry calls with action-scoped options supplied by `WorkspacePanel`.
3. Pass registry snapshot explicitly from `WorkspacePanel` to `TabBar`, `AddTabMenu`, and `AddressBar`.
4. Update service/type imports and store tests so they no longer depend on global registry state.
5. Run focused frontend tests and typecheck/lint where practical:
   - `pnpm --filter app-web test -- workspaceTabStore`
   - `pnpm --filter app-web run typecheck`
   - relevant lint/check command if time allows
