# Canvas 能力完整迁移设计

## 1. 设计原则

1. **能力闭合优先**：以 2026-07-09 老工作树为完整清单，先整块回迁 Canvas feature，
   再在回迁代码内部替换底层接缝。
2. **新事实源唯一**：source、runtime state、resource binding、execution authority 分别只有
   Interaction、Attachment、Operation 对应事实源。
3. **用户态优先**：Canvas 是 Project/User 产品能力；AgentRun 只是可选 attachment 与另一个 caller。
4. **硬切换**：不提供旧路由、旧 DTO、旧 action key、旧 aggregate 或 fallback。
5. **宿主最小权威**：iframe 永远不持有 credential、authority revision、placement、backend id、
   AgentRun id 或本机路径。

## 2. 回迁边界

恢复 `agentdash-workspace-module` 作为 Workspace 产品深模块。其 core 只接管：

- Workspace Module provider 注册、surface 合并与可见性；
- list/describe/operate/invoke/present 通用工具合同；
- exact OperationRef 解析与通用 presentation 路由；
- 系统内嵌模块与未来用户提供模块共用的 descriptor/router/provider contract。

Canvas 是该模块中的一个系统内嵌 provider。Canvas 子模块接管 definition/instance descriptor
projection、create/attach/copy 路由、SourceBundle VFS mount/provider、runtime build input 以及
resource/asset/diagnostic/mailbox bridge。Workspace Module core 不导入 Canvas domain 类型，不查询
Canvas repository，不识别 Canvas mount，也不读写 AgentFrame。

当前 `agentdash-application/src/workspace_module.rs` 与
`runtime_tools/workspace_module_product.rs` 中的同类逻辑迁入该模块，完成后删除重复实现。

不把 Interaction domain/application service、OperationGateway、VFS core 或 AgentRun mailbox
搬进该模块。`agentdash-workspace-module` 只组合这些稳定边界。

## 3. 完整回迁策略

### 3.1 搬运单位

从 reference worktree 一次性取回以下闭合集合：

- backend `canvas/` 与 `workspace_module/` 文件结构；
- `workspace_module_list/describe/operate/invoke/present` 的工具注册、surface、runtime context、
  runtime bridge、结果投影与测试，包含 `canvas.create/attach/copy`；
- Canvas API/contracts、repository/runtime-state 文件，即使它们随后只作为适配脚手架；
- frontend Canvas manager、files editor、bindings editor、runtime preview、runtime SDK 与测试；
- `canvas-system` Skill 及全部 references；
- route/service/contract 测试中体现的用户行为。

第一阶段只做原文件硬搬和台账核对：保留文件职责、工具名称/参数/返回、错误路径、
generation fencing、asset cleanup 与测试场景，允许 Rust/TypeScript 编译直接失败。此阶段不改写
旧 domain import、不提前删除当前实现，也不把旧 route/provider/bootstrap 注册进生产启动链。

### 3.2 适配顺序

1. 对 `research/legacy-tool-behavior-matrix.md` 与 transplant ledger 做搬运完整性签核。
2. Canvas entity/repository → InteractionDefinition/Revision/SourceBundle。
3. Canvas create/attach → authoring mount materialization；当前 Product resource surface 的
   immutable AgentFrame 写入仅作为底层适配细节。
4. Canvas runtime snapshot → definition revision + optional instance/attachment projection。
5. Canvas VFS provider → revision CAS provider。
6. RuntimeGateway descriptors/invoke → OperationGateway surface/exact OperationRef。
7. Session bridge → user/interaction/attachment host。
8. runtime state/snapshot table → canonical Interaction state + renderer observation。
9. old data binding → ResourceSlot/runtime binding。
10. old Agent input route → AgentRun mailbox Operation/provider。
11. 行为测试通过后删除旧 production path、当前残缺替代实现和重复 projection。

搬运、接线、删除是明确分开的工作阶段；不能为了尽快恢复编译而在搬运阶段省略旧文件或测试。

## 4. Operation 权限架构

### 4.1 唯一执行链

```text
Canvas iframe
  -> versioned MessageChannel request
  -> trusted Canvas host
  -> BoundOperationHost
  -> OperationGateway current surface resolution
  -> exact OperationRef admission/schema/placement/audit
  -> provider
```

旧 RuntimeGateway 不恢复。其 request/result、trace、timeout、cancel、readiness 与 provider
错误语义映射到 OperationGateway 的现有合同。

### 4.2 用户与 Agent 分离

- standalone definition/instance 使用 authenticated user principal。
- attached Canvas 中的用户点击仍使用 authenticated user principal。
- Agent 工具调用使用 `AgentRunAgent` principal。
- AgentRun attachment 只约束资源、mailbox、presentation 与 actor-local binding。
- 每次 invoke 重新解析 current authority；iframe 不缓存 authority revision。

因此用户退出 Agent 会话后，仍可用自己的 Project/Interaction 权限继续使用 Canvas。
只有 `agent.submit` 与 attachment-local resource 会明确变为 unavailable。

### 4.3 Operation surface

新增/补齐三类 canonical provider：

- Project Canvas authoring：create/list/copy。
- Definition authoring/distribution：read source、commit changeset、publish、unpublish、
  archive、promote extension、create/present instance、inspect diagnostics。
- Instance runtime：declared command/event、resource bind/unbind、read state projection。

OperationRef 必须 provider-qualified、contract-versioned；definition/instance 的运行命令继续
固定到 exact definition revision。Workspace Module 只投影 Operation catalog，不维护第二份 action
catalog。

Agent-facing `workspace_module_invoke` 仍保持旧的
`module_id + operation_key + input` 参数合同。可信 Workspace Module host 根据 current descriptor
把 `operation_key` 解析成 exact OperationRef 后调用 OperationGateway；Agent 不需要传内部
provider/authority 标记。

## 5. Canvas Host SDK v1

### 5.1 Transport

- `MessageChannel` 独占 port，不使用开放式 window message 作为业务通道。
- connect/initialize/ready/dispose 生命周期。
- envelope 包含 contract version、request id、frame id、generation、kind、payload。
- host 按 generation 丢弃旧请求/结果；reload/unmount 取消请求并 revoke asset URL。
- schema、payload size、rate、timeout 与 outstanding request 数量有界。
- iframe 保持 `sandbox="allow-scripts"`、no-referrer 与明确 CSP。

### 5.2 SDK 能力

`window.agentdash` 继续作为 Canvas 产品 SDK namespace，但采用新合同：

- `operations.list()/describe()/invoke(exactOperationRef, input)`
- `assets.url(uri)/revoke(url)`
- `interaction.getState()/dispatch(commandRef, payload, expectedRevision)`
- `interaction.emit(eventType, payload, expectedRevision)`
- `agent.submit(input)`
- `diagnostics.report(observation)`

保留的是产品能力，不保留旧 `invoke(actionKey, input)` 或任意 `setState(key, value)` 协议。
旧 `extensions.invoke(channelKey, method, ...)` 不再作为旁路 namespace；Extension protocol channel
与 backend service 由 Extension Operation provider 投影为 exact OperationRef 后通过
`operations.invoke` 调用。

### 5.3 Projection

initialize/projection 仅包含：

- definition/instance public identity 与 pinned revision；
- allowlisted canonical state；
- Operation descriptors 的浏览器安全投影；
- declared resource slot projection；
- SDK feature availability；
- theme/locale/viewport；
- opaque presentation/renderer generation。

## 6. Source 与 VFS

### 6.1 Source 唯一事实

Canvas source 只存于 immutable SourceBundle revision。没有 mutable file table 或 Canvas aggregate。

### 6.2 VFS provider

- 每个可见 definition 暴露稳定 Canvas authoring mount。
- personal editable source：read/list/search/apply_patch/write/delete/rename。
- project shared source：read/list/search。
- 无 exec。
- 每次 mutation 原子生成一个新 revision；provider 内部 load-current + changeset + CAS。
- `fs_apply_patch` 由 provider 直接处理整包变更，避免 read/write 间丢失并发更新。
- CAS conflict 原样返回，Agent 重新读取后再编辑。
- resource binding materialized files 可读但不可写。

## 7. Runtime、Interaction 与资源

### 7.1 Runtime build

回迁旧 preview builder 的多文件构建能力：

- TS/TSX/JS/JSX ESM transpile；
- CSS 汇集；
- JSON module；
- local import resolution；
- React/import map；
- build error projection。

definition preview 与 instance runtime 共用 builder 和 SDK，只在 projection/host capability 上裁切。

### 7.2 Interaction state

- current state 是 InteractionInstance canonical state。
- SDK 只能 dispatch declared command/event。
- command 带 command id、expected state revision 与 schema-valid payload。
- Agent 只读 `agent_projection` allowlist。
- presentation 偏好与 renderer diagnostics 不进入 canonical state。

这保留旧 selection/form/filter/recent-event 的可观察能力，同时消除任意 iframe 快照成为事实源。

### 7.3 Resource binding

旧 `canvas.bind_data` 映射为 ResourceSlot binding：

- definition default：可复用的默认资源声明；
- instance shared：所有参与者共享的 runtime resource；
- attachment local：只对当前 actor/AgentRun attachment 生效。

绑定只改变 binding authority，不隐式改 source 或 state。文本绑定可按声明 materialize 到
`bindings/`；二进制由 asset broker 提供 URL。

### 7.4 Asset broker

- 只解析当前 user/instance/attachment applied resource surface 可见 URI。
- 检查 MIME、大小与 slot/binding authority。
- 返回 host 管理的 revocable object URL/opaque asset response。
- binding/attachment/generation 失效后旧 URL 必须失效。
- 不向 iframe 暴露 mount implementation、signed URL、header 或本地路径。

## 8. Diagnostics 与 Agent submit

### 8.1 Renderer observation

新增独立、有界 latest observation：

- instance id、presentation key、renderer lease id/revision、generation；
- ready/error/build_error；
- viewport；
- allowlisted DOM summary；
- runtime diagnostics；
- observed_at。

只有 active renderer lease/current generation 可写。Agent/User 查询仍重新鉴权。该事实不进入
Interaction state，需要数据库时新增独立 migration。

### 8.2 Agent submit

`agent.submit` 由 attached surface 暴露的 exact mailbox Operation 承接：

- queue 对应 canonical submit-input/delivery；
- steer 对应当前 runtime command availability；
- client command id 幂等；
- 可选择附带 current interaction projection 与 renderer observation ref；
- attachment currentness、run status 与 command availability 在执行时重新验证。

standalone Canvas 不隐藏该差异：SDK feature 标记 unavailable，调用返回稳定 unavailable error。

## 9. Workspace Module provider 与用户 UI

- Workspace Module core 通过通用 provider contract 合并 descriptor，并按 route key 把 operate、
  invoke 与 present 交给 owning provider；它不理解 Canvas 资产与 mount。
- 系统内嵌 Canvas provider 投影 `builtin:canvas`、`canvas:{definition_id}` 与
  `interaction:{instance_id}`，并分别承接 authoring、definition、runtime 行为。
- Agent-facing 工具原样保持
  `workspace_module_list/describe/operate/invoke/present`；operate 原样保持
  `canvas.create/attach/copy`。
- 用户 Canvas manager 恢复个人/项目共用、完整 files editor、bindings、preview、发布/复制/
  取消发布/归档/晋升。
- Agent list/describe/invoke/present 与用户 UI 使用同一 Operation descriptors 和 permission projection。

## 10. authoring mount 与 presentation 边界

### 10.1 create/attach 是唯一物化入口

Agent-facing 工具保持旧合同，Canvas provider 内部只有两条 authoring mount command：

| Agent 操作 | 资产变化 | 内部 mount command |
| --- | --- | --- |
| `canvas.create` | 创建 personal definition + first revision | create |
| `canvas.copy` | 从 source 创建 personal definition + first revision | 复用 create |
| `canvas.attach` | 无资产变化 | attach |

create/attach command 执行：

1. 从 Product tool context 取得 authoritative AgentRun target 与 RuntimeThread coordinate。
2. 校验 definition/current revision、Project/user/Agent authority。
3. 构造包含稳定 Canvas authoring VFS mount 的 desired Product resource surface。
4. desired surface 已满足时幂等返回；否则通过当前底层持久化新的 applied surface。当前实现若
   通过 committed immutable AgentFrame 构造该 surface，该事实仅留在 Product resource adapter。
5. CAS 更新 applied Product resource surface，并走当前 Product runtime surface
   rebind/prepare/activate/convergence，而不是恢复旧 direct runtime adopter。
6. 工具完成结果保持旧 Canvas/mount 合同，不返回 AgentFrame；中途失败由同一
   invocation/idempotency receipt 续跑，不能重复创建资产。

### 10.2 其它操作不写 Frame

- `workspace_module_present` 只校验 visible module/view，提交或重放 presentation
  intent/change/outbox 并返回 receipt。
- present 为 renderer 建立的 Interaction instance/attachment 是 presentation/runtime 事实，
  不等于 Canvas 进入 AgentFrame。
- `canvas.bind_data` 只更新 ResourceSlot/applied resource authority。
- Source/VFS mutation 只生成新的 definition revision。
- inspect/get-state/list/describe 是读取。

旧 `CanvasVisibilityRequested` 仅作为被搬回代码中的历史内部类型保留到适配阶段；最终领域和
Agent-facing 术语都统一使用 create/attach，不创造 request 命令。

## 11. 数据库

优先复用现有 Interaction definition/instance/attachment/binding/presentation 表。只在确有 durable
查询需求时新增 renderer observation 表及其 repository。migration 直接建立目标 schema，不迁移或
兼容旧 Canvas 表。

## 12. 删除门槛

完成迁移后以下生产引用必须为零：

- `RuntimeGateway` Canvas execution path
- old Canvas aggregate/repository/runtime state
- old Canvas route/DTO/generated contract
- string action-key iframe invocation
- 双 action catalog、双 source store、双 runtime state

删除只能发生在硬搬台账完成、对应新底层行为测试通过之后。

## 13. 任务组织

本任务保持单一 cross-layer hard-cut，而不拆成可独立上线的子任务。原因是 backend module、
browser SDK、权限 host 与 Agent surface 任一单独交付都会形成残缺 Canvas。实施按 work package
串行设 gate，可由不同实现者处理互不重叠文件，但最终以一次能力闭合验收为准。
