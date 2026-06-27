# Contract Surface Items

## M01 Project event NDJSON contract 化

- Scope: `agentdash-contracts`, `agentdash-api/src/stream.rs`, `packages/app-web/src/api/eventStream.ts`, `packages/app-web/src/types/acp.ts`, `eventStore`.
- Acceptance: Project event stream envelope 由 Rust contract 生成 TS；前端不再维护手写 Project stream union/parser。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

## M02 ProjectBackendAccess / BackendWorkspaceInventory contract 化

- Scope: backend access API DTO、workspace candidate/inventory response、frontend workspace routing types。
- Acceptance: Rust contract + generated TS 覆盖 access、inventory、candidate、sync response。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

## M03 Canvas CRUD contract 化

- Scope: Canvas list/create/update/delete HTTP request/response 与 frontend canvas service。
- Acceptance: Canvas CRUD response 进入 generated contracts；frontend service 删除 raw identity/default mapper。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

## M04 SkillAsset HTTP DTO contract 化

- Scope: SkillAsset list/create/update/import response、frontend `skillAsset` service、editor draft/view model 分层。
- Acceptance: HTTP DTO generated；markdown/frontmatter editor draft 保持 feature-local。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

## M05 ExtensionManagement service 回到 generated DTO

- Scope: `services/extensionManagement.ts` 与 generated `extension-management-contracts.ts`。
- Acceptance: service 直接返回 generated DTO，只保留 UI view model mapper。
- Validation: `pnpm run frontend:check`.

## M06 `workspace_module_presented` stream payload contract 化

- Scope: Workspace module presentation HTTP DTO、Backbone/session platform event payload、frontend presentation parser。
- Acceptance: stream payload 与 HTTP present 使用同源 generated DTO。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

## M07 Auth/current-user/identity-directory DTO contract 化或明确 wrapper

- Scope: auth/current-user/identity directory API DTO 与 frontend services。
- Acceptance: app-wide facts 进入 contracts；纯 auth transport wrapper 明确保留在 route-local。
- Validation: `pnpm run contracts:check`, `pnpm run frontend:check`.

