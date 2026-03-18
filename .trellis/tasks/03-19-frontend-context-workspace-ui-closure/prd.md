# 前端承接上下文编排与虚拟工作空间补全

## Goal

围绕后端已经落地的虚拟上下文容器、统一 Address Space、Session Composition 与 Story / Task MCP 编辑能力，规划前端需要补齐的产品层承接内容，并形成一条可执行的前端补全任务线。

这个任务的重点不是立即完成全部前端实现，而是先把“前端现在缺什么、应该先做什么、哪些能力会影响用户真实体验”说清楚，避免后续后端能力已经可用，但前端仍然停留在不可见、不可编辑、不可验证的状态。

## Background

当前后端已经进入一个新的阶段：

- `Project / Story` 级上下文容器已经可以派生成会话级 mounts
- `spec / brief` 这类虚拟容器已经能在 Story / Task session 中被真实读取
- session plan / context composer 已经把 persona、workflow、runtime policy、tool visibility 等信息显式注入到上下文
- Story / Task 级 MCP 工具已经可用，并且真实验证过可以更新 Story 元数据

但前端目前仍有明显缺口：

- Story 页面“上下文”区主要还是围绕旧的 `source_refs / prd_doc / spec_refs / resource_list`
- `context_containers`、`mount_policy_override`、`session_composition_override` 这些新增后端能力在 UI 中基本不可见
- 用户无法直观看到当前 Session 到底挂载了哪些 mounts、各自有哪些能力
- 用户也无法通过前端去明确配置或编辑 Project / Story 的虚拟容器与会话编排信息
- Task / Story 执行页面虽然能跑通，但“为什么 agent 能看到这些内容、它当前受哪些 workflow 约束”还缺少产品化解释

如果不补这一层，后端已经具备的能力对真实用户来说依然是隐形的。

## Problem Statement

当前前端与后端新能力之间存在三类断层：

1. **可见性断层**
   - 后端已经生成 mounts / persona / workflow / runtime policy
   - 前端没有把这些信息稳定展示出来

2. **编辑入口断层**
   - 后端已经支持 `Project / Story` 元数据层面的虚拟容器与 session composition 编辑
   - 前端还没有结构化编辑入口

3. **调试验证断层**
   - 后端已经允许 agent 在运行时读取虚拟容器
   - 前端没有形成“配置 -> 运行 -> 结果验证”的闭环视图

## Requirements

- 盘点当前前端中与 `Project / Story / Task / Session` 相关的页面、store、types，明确哪些地方仍停留在旧上下文字段模型。
- 设计 `Project / Story` 级虚拟容器的前端展示与编辑方案，至少覆盖：
  - 容器列表
  - mount_id / display_name
  - provider 类型
  - capabilities
  - exposure
  - disabled container
- 设计 `session_composition` 的前端展示与编辑方案，至少覆盖：
  - persona_label
  - persona_prompt
  - workflow_steps
  - required_context_blocks
- 设计 Session 页面中“当前运行上下文”的可视化方案，至少覆盖：
  - 当前 mounts 清单
  - 每个 mount 的能力
  - 当前工具可见性
  - persona / workflow / runtime policy 摘要
- 明确 Story / Task 页面中哪些位置应该展示“该 agent 看到的上下文来自哪里”，避免用户只能通过聊天回复倒推系统状态。
- 设计前端对 Story MCP 编辑能力的承接方案：
  - 是直接走 REST 结构化保存
  - 还是提供“通过会话驱动编辑”的调试辅助入口
  - 哪些属于正式产品入口，哪些属于开发调试入口
- 明确前端 types 与 store 需要如何扩展，才能稳定接住后端新增字段，而不是继续在页面里临时拼字段。
- 给出一条前端实施优先级，区分：
  - 必须先补的产品可见性
  - 其次补的编辑能力
  - 最后补的高级调试能力

## Acceptance Criteria

- [ ] 明确 Story / Session / Task 页面当前缺失的上下文编排可视化能力清单。
- [ ] 明确 `context_containers / mount_policy / session_composition` 的前端展示模型。
- [ ] 明确 Project / Story 的正式编辑入口方案。
- [ ] 明确 Session 页面如何展示 mounts、tools、persona、workflow、runtime policy。
- [ ] 明确前端 types / store / services 需要补齐的字段与边界。
- [ ] 明确前端实施优先级与切片建议，能直接拆成后续实现任务。

## Proposed Scope

### 1. Story / Project 配置面板补全

优先考虑把新增后端字段真正纳入正式编辑界面：

- Project:
  - `context_containers`
  - `mount_policy`
  - `session_composition`
- Story:
  - `context_containers`
  - `disabled_container_ids`
  - `mount_policy_override`
  - `session_composition_override`

建议不要继续把这些能力藏在“高级 JSON 输入”里，而是提供结构化表单或卡片化编辑。

### 2. Session 可视化补全

Session 页面需要新增一个“运行上下文”视图，至少包括：

- 当前 mounts
- mount capabilities
- 当前可用工具
- persona 摘要
- workflow 摘要
- runtime policy 摘要

这样用户才能理解：

- 为什么 agent 能看到某个 mount
- 为什么当前没有 `main` workspace
- 为什么某个 session 只有 `spec / brief`

### 3. 执行结果验证辅助

Task / Story session 中，应能快速看到：

- 最近一次工具调用读取了哪个 mount
- 当前回答是基于哪些 mounts / tools 完成的
- 是否成功写回 Story / Task 元数据

这部分既是用户体验，也是后续排障入口。

## Priority Suggestion

### P1: 先补“看见”

- Session 页面增加 mounts / persona / workflow / runtime policy 可视化
- Story 页面增加虚拟容器与编排字段只读展示

### P2: 再补“编辑”

- Project / Story 的结构化编辑入口
- 与 store / types 的系统性补齐

### P3: 最后补“调试辅助”

- 会话页的 mount/tool 调试抽屉
- MCP 编辑结果回显
- 更完整的 session context inspector

## Out of Scope

- 立即实现外部 provider service 的管理后台
- 设计企业级多租户权限 UI
- 重做全部信息架构或导航结构
- 引入新的富文本编辑器体系
- 用前端绕过后端结构化能力直接改内部字段

## Suggested Follow-up Tasks

1. `frontend-session-context-visibility`
   - 先让 Session 页面看得见 mounts / persona / workflow / runtime policy

2. `frontend-story-project-context-editors`
   - 为 Project / Story 增加结构化容器与 session composition 编辑面板

3. `frontend-task-story-context-debug-panel`
   - 提供运行时上下文调试与验证辅助

## Related Files

- `.trellis/spec/backend/address-space-access.md`
- `frontend/src/pages/StoryPage.tsx`
- `frontend/src/pages/SessionPage.tsx`
- `frontend/src/features/story/story-session-panel.tsx`
- `frontend/src/features/task/task-agent-session-panel.tsx`
- `frontend/src/stores/storyStore.ts`
- `frontend/src/types/index.ts`

