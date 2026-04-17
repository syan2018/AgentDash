# Research

## Relevant Specs
- `.trellis/spec/backend/address-space-access.md`: 定义统一 Address Space / mount + path / provider / runtime tool 的目标契约，这次重构的核心后端边界。
- `.trellis/spec/backend/error-handling.md`: 要求后端分层错误语义清晰，避免在 API / application 层继续扩散裸字符串与重复包装。
- `.trellis/spec/backend/quality-guidelines.md`: 要求跨层 DTO 保持 snake_case、一份契约一种真相，禁止前端长期兼容漂移字段。
- `.trellis/spec/frontend/component-guidelines.md`: 前端组件应区分 model 与 ui，通用浏览器不能继续混入业务解析逻辑。
- `.trellis/spec/frontend/type-safety.md`: 前端 mapper 只负责 `unknown -> typed object`，不能再用多套字段/多套摘要掩盖后端契约漂移。
- `.trellis/spec/frontend/state-management.md`: 共享数据应在 store/service 层集中获取，派生展示使用 memo，不应在组件内重复推导跨页面真相。
- `.trellis/spec/guides/cross-layer-thinking-guide.md`: 明确要求跨层功能先定义统一 mount/provider/capability 边界，并验证前端看到的是“真实生效的 runtime surface”。

## Code Patterns Found
- 统一 mount/provider 操作底座：`crates/agentdash-application/src/address_space/relay_service.rs`
- mount 构建与 metadata 标注：`crates/agentdash-application/src/address_space/mount.rs`
- Project session bootstrap/snapshot 汇总：`crates/agentdash-api/src/routes/project_sessions.rs`
- 当前浏览器错误耦合模式（待替换）：`frontend/src/features/address-space/address-space-browser.tsx`
- Session 上下文展示入口：`frontend/src/features/session-context/context-panels.tsx`

## Files to Modify
- `crates/agentdash-application/src/address_space/mount.rs`: 拆分基础 mount 派生与 agent/runtime surface 修饰逻辑，引入独立 knowledge surface 语义。
- `crates/agentdash-application/src/address_space/relay_service.rs`: 新增/承接 resolved surface 解析入口与基于 surfaceRef 的 mount 操作路径。
- `crates/agentdash-api/src/routes/address_spaces.rs`: 用 resolved surface API 替换当前 project/story/owner/agent 坐标式 preview/read/write/list/patch。
- `crates/agentdash-api/src/routes/project_agents.rs`: 删除 `build_project_agent_visible_mounts()` 旁路摘要，改为消费真实 resolved surface；补 Agent Knowledge surface 接口。
- `crates/agentdash-api/src/routes/project_sessions.rs`: 删除 `shared_context_mounts` 二次推导，改为从 runtime surface 派生 snapshot 可见 mount 摘要。
- `crates/agentdash-application/src/session/context.rs`: 替换 `SharedContextMount` 旧模型，改为 surface/surface-derived summary。
- `crates/agentdash-application/src/session/bootstrap.rs`: 统一 bootstrap plan 与 snapshot 的 surface 引用。
- `frontend/src/services/addressSpaces.ts`: 改成 `surfaceRef + mountId + path` API。
- `frontend/src/features/address-space/address-space-browser.tsx`: 重写为纯 resolved surface 浏览器，不再承担 preview 解析。
- `frontend/src/features/project/agent-preset-editor.tsx`: 改成 Agent Knowledge 专用浏览器，只看 `agent-knowledge`。
- `frontend/src/features/project/project-agent-view.tsx`: Project Agent 摘要改用真实 resolved surface 或其派生摘要。
- `frontend/src/features/session-context/context-panels.tsx`: Session/Project 上下文展示改用真实 runtime surface。
- `frontend/src/types/index.ts` / `frontend/src/types/context.ts`: 引入新 surface DTO，移除/淘汰 `shared_context_mounts` 与旧 preview DTO。
