# 当前架构接缝

## 已存在且应直接复用

| 当前能力 | 位置 | 迁移用途 |
| --- | --- | --- |
| Immutable SourceBundle + digest | `agentdash-domain/src/interaction/source.rs` | Canvas source 唯一事实源 |
| Definition revision CAS | `agentdash-application/src/interaction/canvas_definition.rs` | VFS/API/Operation authoring commit |
| Personal/Project owner、publish/copy/archive | Interaction definition service/routes | 用户独立资产管理 |
| Interaction instance + attachment | `agentdash-domain/src/interaction/instance.rs` | shared runtime 与 AgentRun 附着 |
| Canonical state revision/command/event | Interaction command service | 交互状态与并发控制 |
| ResourceSlotDefinition/runtime binding | Interaction definition/instance | 替代旧 `bind_data` |
| Presentation state/renderer lease | `interaction/presentation.rs` | tab 状态、renderer fencing 与 generation |
| Exact OperationRef | `agentdash-domain/src/operation.rs` | 稳定 capability identity |
| OperationGateway | `agentdash-application-operation-gateway` | 唯一执行、鉴权、schema、placement、audit 层 |
| UserWorkshopOperationHost | `operation_hosts.rs` | 无 AgentRun 的用户态 Canvas |
| AgentRunOperationHost | `operation_hosts.rs` | Agent 工具调用 |
| MessageChannel iframe pattern | `ExtensionInteractionComponent.tsx` | Canvas host SDK 协议基线 |
| Workspace Module list/describe/invoke/present | `workspace_module_product.rs` | Agent 发现与交付入口 |
| AgentRun Product command/mailbox | `lifecycle_agents.rs` 与 application-agentrun | submit/steer canonical 落点 |
| Immutable AgentFrame + Product runtime rebind | application-agentrun Product runtime provisioning/convergence | Canvas create/attach 后的稳定 surface 生效链 |

## 当前缺口

1. `CanvasRuntimePanel` 只把 entry source 放入 `srcDoc`，没有构建多文件 runtime。
2. 普通 Canvas iframe 没有 MessageChannel host SDK。
3. 没有 Canvas source VFS mount/provider。
4. Agent 只能看到已有 project-shared definition；无法从空项目创建个人 Canvas。
5. Workspace Module 只有 list/describe/invoke/present；旧 `workspace_module_operate` 及其
   `canvas.create/attach/copy` 已丢失。
6. 没有浏览器 asset broker。
7. 没有 renderer observation/diagnostic durable latest fact。
8. 没有 Canvas-origin AgentRun mailbox host。
9. Resource slot binding 领域对象存在，但缺完整用户/Agent mutation surface。
10. standalone 与 AgentRun-attached runtime 没有统一 Canvas SDK contract。
11. `AgentRunRuntimeSurfaceUpdatePort` 等 AgentFrame surface port 仍有声明，但当前没有对应 production
    updater；Canvas create/attach 的 Frame mutation 实际未接通。

## 权限层结论

当前 Operation 设计已经具备正确收束点：

- `HostOperationInvocation` 不接受 principal、scope、origin、authority revision 或 placement。
- `BoundOperationHost` 由可信宿主绑定这些字段。
- `OperationGateway.invoke` 每次重新解析 current authority surface，再校验 exact OperationRef。
- user、AgentRun agent、workflow、extension 分别进入独立 authority resolver。
- user Canvas 已有 `UserWorkshopOperationHost::canvas`；
  Interaction/attachment 已有对应 host。

因此不恢复旧 RuntimeGateway。旧 Gateway 的宿主协议经验迁到 Canvas host，实际执行统一进入
OperationGateway。

Agent-facing Workspace 工具仍按旧合同接收 `module_id + operation_key + input`；可信 host 用
current descriptor 解析 exact OperationRef。统一 Gateway 不要求改变 Agent 工具参数。

## AgentFrame 调查结论

### 当前事实

- `crates/agentdash-application-ports/src/agent_frame_materialization.rs` 仍声明 runtime-surface update
  与 frame-surface command ports。
- 当前生产树中已不存在原 `product_runtime_surface_update.rs`，也不存在旧
  `agent_run/frame/surface_service.rs`、`runtime_surface_update.rs` 实现。
- 因此不能从 port 或 `workspace_module_present` 当前存在推断 Canvas 已进入 AgentFrame。
- 当前 `workspace_module_product.rs::execute_present` 建立 Interaction instance/attachment；这是
  presentation runtime 事实，不是 AgentFrame materialization。

### 可复用历史实现

- `ef4cc2499` 曾以 immutable AgentFrame revision + AppliedResourceSurface CAS 实现 Product runtime
  surface update。
- `708f234c9` 曾把 `canvas.create/attach/copy` 接到该 updater，并返回 frame/applied surface
  convergence evidence。
- 这些提交比 2026-07-09 的 direct runtime adopter 更接近当前 Product 底层，可作为接线参考；
  公开工具行为仍以 2026-07-09 reference 为准。

### 最终边界

- `canvas.create` 和 `canvas.copy` 创建新 definition，统一复用 create Frame command。
- `canvas.attach` 对既有 definition 使用 attach Frame command。
- create/attach 基于 committed Frame 构造 desired surface，持久化必要的新 revision，并沿当前
  Product runtime rebind/convergence 生效。
- `workspace_module_present`、`canvas.bind_data`、source edit、list/describe/inspect/state 和
  Interaction attachment 都不调用 AgentFrame updater。
- 旧内部类型 `CanvasVisibilityRequested` 只作为硬搬阶段证据；最终不引入 request 新术语。

## Actor 模式

| 场景 | Principal | Scope | Origin | Attachment |
| --- | --- | --- | --- | --- |
| 用户独立预览 definition | 当前用户 | Project | Canvas(definition) | 无 |
| 用户独立使用 instance | 当前用户 | InteractionInstance | Interaction(instance) | 可无 |
| 用户在 Agent workspace 点击 Canvas | 当前用户 | InteractionInstance | Interaction(instance) | 当前 attachment |
| Agent 调用 Canvas authoring | AgentRunAgent | Project | AgentTool | 无 |
| Agent 调用 Interaction command | AgentRunAgent | InteractionInstance | AgentTool | 当前 attachment |

浏览器内用户点击不冒充 Agent principal。AgentRun attachment 只增加资源面、mailbox 与交付上下文，
不成为通用 Operation 权限的事实源。

## VFS 结论

当前 generic MountProvider mutation 没有 caller-provided expected version，但 `fs_apply_patch`
可以由 Canvas provider 整体接管。目标 provider 在单次 mutation 内：

1. 读取 current definition revision；
2. 在 current SourceBundle 上应用完整 patch；
3. 用 current revision id 调用 definition CAS commit；
4. 成功后产生一个新 immutable revision；
5. 并发冲突直接返回 conflict，不静默重试或覆盖。

这保留旧 VFS 编辑体验，同时不需要 draft aggregate。`read_text` 返回 definition revision/digest
组成的 version token；generated resource binding 文件只读；mount 不支持 exec。

## 需要新增的数据事实

Renderer observation 与 diagnostics 不属于 canonical Interaction state，也不应塞进 presentation
偏好。应新增有界、按 instance + presentation + renderer lease/generation 定位的 latest observation
事实，并通过 current attachment/user authority 查询。若采用数据库持久化，需要新增 migration，
不复用已删除的 AgentRun Canvas runtime state 表。
