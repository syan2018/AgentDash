# 统一 Address Space Surface 全量重构

## Goal
将当前分散在前端组件、API 路由、Session Snapshot、Project Agent 摘要中的多套 mount 推导逻辑，收敛为唯一的 `ResolvedAddressSpaceSurface` 真相源；所有浏览、预览、读写、摘要展示都只能消费这份已解析 surface。

## Requirements
- 建立统一的 Address Space Surface 解析模型，显式区分 `ProjectPreview`、`StoryPreview`、`TaskPreview`、`SessionRuntime`、`ProjectAgentKnowledge` 等 source。
- Agent 页的知识库浏览入口必须只展示当前 `ProjectAgentLink` 私有知识库 mount，不允许混入 project/workspace/lifecycle/canvas 等任何其它 mount。
- 前端通用 mount 浏览器不得再接收 `projectId / storyId / ownerType / ownerId / agentId` 等业务参数做二次推导；它只能消费后端已解析 surface。
- 后端 mount 读写/列表/搜索/patch 接口必须改为基于 `surfaceRef + mountId + path` 工作，不再接受用于重建 address space 的业务坐标。
- Project Agent 摘要、Project Session context snapshot、Session 页 Address Space 面板必须全部基于同一份 resolved surface 派生展示，禁止继续手工重算 `shared_context_mounts` 或其它平行摘要。
- 删除或替换当前误导性的中间 DTO / 旁路逻辑，包括但不限于 `shared_context_mounts`、`build_project_agent_visible_mounts()`、`AddressSpaceBrowser.preview` 业务参数模式。
- 保证 inline knowledge mount 的读写持久化仍然基于 `InlineContentOverlay + InlineContentPersister`，但 UI 侧不再感知 owner/project/story/agent 解析细节。

## Acceptance Criteria
- [ ] 后端存在统一的 surface 解析入口与稳定 DTO，能够为 Project/Story/Task/Session/Agent Knowledge 五类 source 生成唯一 mount surface。
- [ ] Agent 页“浏览知识库”只显示 `agent-knowledge` mount，且不能切换到任何其它 mount。
- [ ] Session 页与 Project Agent 摘要展示出的 mount 信息与真实 runtime address space 完全一致，不再出现第二套推导结果。
- [ ] 前端通用浏览器与服务层 API 全部切换到 `surfaceRef + mountId + path` 模型。
- [ ] 旧的 `shared_context_mounts` 旁路推导和 `AddressSpaceBrowser.preview` 业务坐标模式已删除或不再被任何调用方使用。
- [ ] 关键路径具备测试覆盖：Agent Knowledge surface 只返回单 mount；SessionRuntime surface 正确反映白名单过滤与知识库注入；浏览器读写路径不再依赖业务参数重解。

## Technical Notes
- 这次重构是明确的跨层任务，涉及 `agentdash-application` 的 address space 构建、`agentdash-api` 的 surface/query/read-write 路由、`frontend` 的浏览器组件与 Agent/Session UI 展示链路。
- 该项目当前未上线，不做兼容层，不保留旧接口长期并存；迁移完成后应直接删除旧模型与旧调用路径。
- `ProjectAgentKnowledge` 不是 `ProjectPreview + agent_id filter` 的特例，而是独立 source，语义是“这个 link 私有知识库文件系统”。
- 所有 mount 摘要都必须从 resolved surface 派生，而不是从 project/story container 配置二次手工计算。
