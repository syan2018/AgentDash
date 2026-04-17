# Migration Plan

## Target Model

### Single Source of Truth
- 系统中只允许存在一种“可浏览/可编辑的挂载面”真相：`ResolvedAddressSpaceSurface`
- 所有 mount 列表、默认 mount、能力、owner 归属、purpose、在线状态、文件数等信息只能从 resolved surface 获取
- 任何模块不得再自行根据 `project/stor/agent/session` 配置二次推导并生成平行摘要

### Surface Sources
- `ProjectPreview`
- `StoryPreview`
- `TaskPreview`
- `SessionRuntime`
- `ProjectAgentKnowledge`

### Browser Contract
- 通用浏览器只接收：
  - `surface_ref`
  - `mounts`
  - `default_mount_id`
  - 可选 `visible_mount_ids`
- 通用浏览器不接收：
  - `project_id`
  - `story_id`
  - `owner_type`
  - `owner_id`
  - `agent_id`

### File Operation Contract
- 所有 list/read/write/search/apply_patch 接口统一改为：
  - `surface_ref`
  - `mount_id`
  - `path`
  - `content/patch`
- 业务坐标只允许出现在 “resolve surface” 入口，不允许出现在通用 mount 操作接口

### Agent Knowledge Contract
- Agent 页知识库浏览入口解析的必须是 `ProjectAgentKnowledge` source
- 该 source 返回的 surface 只能包含一个 mount：`agent-knowledge`
- `project_container_ids` 只作用于 session/runtime surface，不作用于 Agent Knowledge surface

## Full Refactor Tasks

### Backend Application
- 新增统一 `ResolvedAddressSpaceSurfaceService`
- 抽离 surface source 到显式枚举与 DTO
- 将 `mount.rs` 中“基础 mount 构建”和“agent/runtime 修饰”拆分
- 为 `ProjectAgentKnowledge` 提供独立 surface 构建逻辑
- 为 surface 构建结果统一派生 mount 摘要

### Backend API
- 替换当前 `/address-spaces/preview` 坐标式 preview 接口
- 新增 surface resolve / query / mount-operation 接口
- 删除 route 内部手工拼装 `shared_context_mounts` 的逻辑
- 删除 `build_project_agent_visible_mounts()` 摘要旁路
- 让 ProjectAgentSummary / Session snapshot / runtime browse 全部消费统一 resolved surface

### Frontend Services & Types
- 引入 `ResolvedAddressSpaceSurface`、`ResolvedMountSummary`、`surface_ref` 等新 DTO
- 删除旧 preview DTO 与基于业务坐标的文件操作参数模型
- 更新 `addressSpaces.ts` 为 surfaceRef 模型
- 更新 `types/index.ts` / `types/context.ts`

### Frontend UI
- 将 `AddressSpaceBrowser` 改写为纯 surface 浏览器
- 新增面向业务的 surface 容器层，而不是把业务解析塞进通用浏览器
- Agent 页改为 `AgentKnowledgeBrowser`
- Session / Project / Story 面板统一消费 resolved surface

### Snapshot & Summary Cleanup
- 删除 `SessionOwnerContext::Project.shared_context_mounts`
- 改为 surface-derived summary 或直接使用 runtime surface
- 保证 Project Agent 摘要与 Session 页展示不再出现第二套 mount 真相

### Testing
- 覆盖五类 surface source 的构建测试
- 覆盖 Agent Knowledge surface 单 mount 约束
- 覆盖 SessionRuntime surface 白名单过滤 + knowledge 注入
- 覆盖前端 Agent 页只能浏览 knowledge mount
- 覆盖 mount list/read/write 不再依赖业务坐标重解

## Deletions Required
- 删除 `AddressSpaceBrowser.preview` 业务坐标模式
- 删除 `build_project_agent_visible_mounts()`
- 删除 `SharedContextMount`
- 删除 `project_sessions.rs` 中对 `shared_mounts` 的二次拼装
- 删除通用 mount API 中的 `project_id/story_id/owner_type/owner_id/agent_id` 入参

## Done Criteria
- 所有浏览/编辑/摘要/UI 展示都只来自同一份 resolved surface
- Agent 页知识库入口永远不可能切到 project/workspace/lifecycle/canvas mount
- 不再存在任何第二套 mount 推导逻辑
