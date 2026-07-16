# Runtime Surface 来源与 Permission 清理设计

## 1. 设计结论

本任务将两个问题分别收敛，再通过明确 owner 消除它们之间的错误耦合：

1. Surface adoption 只依赖 revision、digest、CAS、operation identity 与 normalized delta；Workflow phase 是可选展示事实。
2. Permission 只在执行边界通过 AgentRun 判定；它不是 Business Surface contribution。

当前产品只保留一个默认允许的 AgentRun 权限门面。动态审批的 RuntimeInteraction 能力继续存在，但当前默认策略不会触发。长期 Grant 的实现延后；未来它属于运行中的 `LifecycleRun` 聚合，并只由 AgentRun 层暴露。

## 2. 第一性原理

### 2.1 Surface adoption 的必要事实

一次 adoption 必须证明：

- 目标 AgentFrame 与 Surface revision 是什么；
- previous revision/digest 是否仍是当前基线；
- 同一个 operation 重放是否幂等；
- Driver 应用哪个 binding generation 与 surface/tool/hook digest；
- previous/target normalized surface delta 是否确定。

这些问题由以下事实完整回答：

- source frame id/revision；
- previous/target surface revision 与 digest；
- runtime operation/idempotency identity；
- binding id/generation；
- normalized surface delta。

Workflow `node_path` 不参与以上任何不变量，所以只能是 presentation metadata。

### 2.2 Permission 的必要事实

Permission 回答的是：

> 某个 AgentRun 中的执行主体，是否被允许在当前动作发生前执行该动作？

它不回答：

- 工具是否存在；
- capability 是否支持该工具；
- VFS path 是否可达；
- credential 是否可解析；
- binding generation 是否有效；
- Hook 是否 rewrite 或 block。

这些约束已经有各自 owner。Permission 只消费它们归一化后的动作事实，不复制其状态。

### 2.3 Grant 与 Approval 不是同一对象

- Grant：长期授权事实，属于 AgentRun 对应的 `LifecycleRun` 运行实例。
- RuntimeInteraction：Runtime 暂停执行并等待 Host 回答的一次交互。
- Approval request：尚未产生 Grant 的待决问题。
- Approval response：允许时可由 AgentRun 创建/更新 Grant；拒绝时不产生 Grant。

因此不需要让 Grant 自己经历 created、pending_policy、pending_user、approved、applied、failed、expired、revoked、scope_escalated 等完整审批状态机。

## 3. Ownership 与依赖方向

### 3.1 对照原始架构任务

| 原始任务方向 | 本任务结论 | 原因 |
| --- | --- | --- |
| AgentRun 是产品运行容器与授权边界 | 保留并强化 | Grant 的所有产品读写都必须经 AgentRun |
| RuntimeInteraction 承载 approval/user input/elicitation | 保留 | 动态审批是确定需求，Interaction 是正确的等待原语 |
| Tool Broker 每次调用重新验证 permission | 保留并窄化 | 每次执行都调用 AgentRun permission facade，但不读取 Grant repository |
| Driver 不读取 Grant repository | 保留并扩展到所有外层 | Runtime、Adapter、Surface、API、UI 同样不得绕过 AgentRun |
| Permission/policy contribution 进入 immutable Business Surface | 删除 | 长期 Grant 是运行时授权事实，不是 Agent 可见能力表面；每次判定比 Surface revision 更直接 |
| PermissionGrant 作为独立业务聚合 | 修正 | Grant 与运行实例同生命周期，应是 `LifecycleRun` typed field |
| Grant apply/revoke 通过 AgentFrame/Surface adoption 生效 | 删除 | 授权变化不应创建模型上下文 revision，也不应依赖 Workflow provenance |
| canonical Runtime 使用 owned vocabulary | 保留并补完 | 当前 vendor approval DTO 与 temporary request 必须清理 |

原始任务最有价值的部分是 owner 与执行边界；未完成的部分是没有把 Grant 的生命周期、持久化归属和唯一暴露面收紧，导致后续实现横向扩散。

| 对象/能力 | Owner | 对外边界 |
| --- | --- | --- |
| `LifecycleRun` 与未来 Grant document | Workflow domain + persistence | 只供 AgentRun application implementation 使用 |
| AgentRun 权限验证、未来 Grant CRUD/read model | AgentRun application | protocol-neutral AgentRun permission facade |
| approval 等待、回答、恢复 | Managed Runtime | RuntimeInteraction，经 AgentRun runtime facade 协调 |
| brokered tool 执行 | Tool Broker | 调用 AgentRun permission facade，不读取 Grant |
| vendor command/file/permission request | Driver/Adapter | 映射 owned request，调用 AgentRun permission facade |
| Capability/VFS/Hook/Credential | 各自模块 | 各自校验，不承载 Grant |
| HTTP/UI | AgentRun product surface | 当前不暴露 Grant；未来只使用 AgentRun-scoped API |

硬依赖规则：

```text
Tool Broker / Managed Runtime / Driver / Adapter / Surface / API / UI
                              |
                              v
                 AgentRun Permission Facade
                              |
                              v
         LifecycleRunRepository + Runtime Gateway
```

Grant model、Grant persistence shape 与 `LifecycleRunRepository` 不能穿过 AgentRun facade 向外泄漏。

## 4. Runtime Surface 目标模型

### 4.1 phase 降级为可选展示元数据

`RuntimeSurfacePresentationPlan::for_adoption` 不再要求非空 `transition_phase_node`。

规则：

- Workflow node transition 可以携带 `Some(node_path)`；
- Canvas、VFS、MCP、Skill、Workspace Module 等 live update 可以携带 `None`；
- `None` 不阻止 delta projection、plan compile 或 adoption；
- 不伪造 `canvas_expose`、`permission_apply` 等 Workflow phase。

### 4.2 ContextFrame projection

projection 由 normalized delta 决定：

```rust
project_live_surface_transition(
    previous,
    target,
    identity,
    PresentationMetadata {
        phase_node: Option<&str>,
        mode: Live,
    },
)
```

展示规则：

- 有 phase：可显示 Step Transition 与 node path；
- 无 phase：显示通用 Runtime Surface Update；
- frame identity 只使用稳定 operation/source frame/revision/ordinal；
- timestamp 不参与 identity；
- 相同 operation 重放产生相同 frame IDs 与 digest。

当前不新增庞大的 `SurfaceChangeCause` union。业务 request 已经是 typed source；Runtime presentation 只需要 delta、stable identity 与可选 metadata。

### 4.3 写入与采用顺序

当前 Canvas 路径先 `frame_repo.create`，再调用 adoption。首先应修复并验证所有已知 deterministic adoption 前置错误；只有测试证明失败 revision 会被后续查询当成 active current 时，才引入独立 adopted pointer。目标不变量是：

1. 从当前 AgentRun surface 构造 next frame；
2. 能前移的 HookPlan、Business Surface、normalized delta 与 presentation plan 确定性校验在持久化前完成；
3. durable accept canonical `SurfaceAdopt` operation；
4. Runtime accept 后，新 frame 才能作为 active surface 被消费；
5. Driver apply failure由既有 operation recovery 收敛。

关键区分：

- deterministic compile failure：发生在公开 current 之前；
- durable accept 后的 Driver failure：保留 accepted operation 并恢复，不能回滚 canonical fact；
- 未采用 frame 不能因“最高 revision”自动成为 active current。

是否需要独立 candidate/adopted read model 由失败注入测试决定；不为尚未出现的并发模型预建第二套状态机。

## 5. 当前 AgentRun 权限门面

### 5.1 最小契约

权限入口需要为 future Grant repository I/O 留出正确形态，因此采用 async port；当前 allow-all implementation 仍然是零状态逻辑。

概念契约：

```rust
#[async_trait]
pub trait AgentRunPermissionFacade: Send + Sync {
    async fn authorize(
        &self,
        request: AgentRunPermissionRequest,
    ) -> Result<AgentRunPermissionDecision, AgentRunPermissionError>;
}

pub enum AgentRunPermissionDecision {
    Allowed,
    Denied { reason: String },
    PendingApproval { interaction_id: RuntimeInteractionId },
}
```

`AgentRunPermissionRequest` 只包含当前执行点已经拥有的 canonical facts：

- `run_id`、`agent_id`；
- runtime thread/turn/item/tool-call coordinates；
- 动作类别：brokered tool、command execution、file change、permission profile；
- rewrite 和路径归一化后的 action/resource summary；
- binding generation 或幂等 coordinate。

它不包含：

- Grant status；
- TTL；
- policy revision；
- UI identity；
- vendor generated DTO；
- AgentFrame/Surface snapshot；
- repository handle。

### 5.2 当前 production behavior

当前实现：

```rust
pub struct AllowAllAgentRunPermissionFacade;
```

它固定返回 `Allowed`。当前任务不读取 `LifecycleRunRepository`，不建立 Grant document，不创建 RuntimeInteraction，不产生 UI。

保留 `Denied` 与 `PendingApproval` 的原因不是提前实现 Grant，而是这两个结果是一个真实权限验证入口不可缺少的协议语义，并且动态审批已被明确确认。

### 5.3 契约放置

依赖方向要求 Tool Broker 与 Integration 不能反向依赖 `agentdash-application-agentrun`。实施时应：

- 将 protocol-neutral request/decision/port 放在 Tool Broker、Driver Host 与 AgentRun 均可依赖的 dependency-light contract/SPI；
- 将唯一 production implementation 与未来 Grant/Interaction coordination 放在 `agentdash-application-agentrun`；
- 在 composition root 注入该 implementation；
- 禁止 Runtime 或 Integration 提供第二套 policy implementation。

“contract 位于低层依赖包”不等于 ownership 下沉；业务 owner 仍是 AgentRun，低层只承载无状态端口。

## 6. Tool Broker 与 Driver 数据流

### 6.1 Tool Broker

推荐顺序：

```text
binding/generation
-> tool catalog/capability
-> BeforeTool rewrite/block
-> VFS/path normalization and feasibility
-> AgentRun permission authorize
-> credential resolution
-> durable Running
-> executor side effect
-> AfterTool
-> terminal
```

原因：

- 无效工具、不可达 VFS 不应触发审批；
- permission 必须基于 rewrite 后的最终动作；
- credential 与 side effect 必须位于 permission allow 之后。

清理结果：

- Tool Broker 不查询 Grant repository；
- Tool Broker 不把 capability admission 复制成 permission policy；
- Tool Broker 不把 Grant 合并进 VFS；
- `PendingApproval` 时只保存 canonical interaction coordinate 并等待 AgentRun resolution。

### 6.2 Driver/vendor approval

Codex 等 Driver 的 command/file/permission request：

1. Adapter 映射为 AgentDash-owned `AgentRunPermissionRequest`；
2. Driver Host 调用注入的 AgentRun permission facade；
3. `Allowed`：立即返回 vendor allow；
4. `Denied`：返回 vendor deny；
5. `PendingApproval`：保持 vendor request pending，由 AgentRun/RuntimeInteraction 完成 resolution。

Adapter 不读取 Grant，不解释 AgentRun policy，也不把 vendor DTO写入 canonical Runtime。

## 7. RuntimeInteraction 与动态审批

### 7.1 当前保留范围

RuntimeInteraction 是 Managed Runtime 的通用 durable primitive，至少继续支持：

- command approval；
- file change approval；
- permission approval；
- user input；
- MCP elicitation；
- dynamic tool execution。

当前清理只做：

- 用 AgentDash-owned request/response 替换 Codex generated approval DTO；
- 删除 `temporary_permission_approval` 等伪造 request；
- 确保 permission interaction 只能由 AgentRun permission coordination 产生；
- 当前 allow-all production 不产生 pending approval。

### 7.2 未来批准流程

未来实现 Grant 时，唯一协调者是 AgentRun：

```text
execution boundary
-> AgentRun.authorize
-> no matching Grant
-> AgentRun creates RuntimeInteraction
-> user resolves through AgentRun command
-> AgentRun persists Grant in LifecycleRun
-> AgentRun resolves RuntimeInteraction
-> execution resumes
-> AgentRun.authorize re-check
-> side effect
```

批准顺序采用：

1. 以 idempotency key 与 expected LifecycleRun version 持久化 Grant；
2. 再 resolve RuntimeInteraction；
3. 第二步失败时由 AgentRun reconciliation 重试；
4. 执行恢复后再次 authorize，确保未持久化 Grant 时不会发生 side effect。

拒绝不产生 Grant，只 resolve interaction 为 denied。

这样 Grant 是唯一长期授权事实，RuntimeInteraction 是唯一等待事实，不需要跨聚合伪原子事务。

## 8. 未来 Grant 的最小边界

### 8.1 持久化归属

未来推荐：

```text
lifecycle_runs
  id
  ...
  grants jsonb NOT NULL DEFAULT '[]'
```

domain 中对应：

```text
LifecycleRun
  ...
  grants: LifecycleRunGrantDocument
```

它是同一运行实例聚合的 typed field，而不是 generic metadata。

理由：

- Grant 与运行实例同生命周期；
- Run 删除时自然删除；
- Run 版本/CAS 可以保护 Grant mutation；
- 不需要独立 join、foreign key、repository、全局 API 与级联清理；
- 与当前 `orchestrations/tasks/execution_log/channel_registry` 的同行 JSONB 模式一致。

本任务不添加该字段。

### 8.2 CRUD 与并发

未来 Grant mutation 必须通过 `LifecycleRunRepository` 的窄化原子 mutation 能力完成，类似当前 `mutate_channel_registry`：

- row lock 或 expected version/CAS；
- 读取当前 document；
- 执行 typed mutation；
- 同事务写回；
- 返回最新 document。

该 repository capability 只注入 AgentRun application implementation。其他模块不能获得 Grant mutation handle。

### 8.3 AgentRun 唯一暴露面

未来可能出现的：

- list grants；
- approve and create grant；
- revoke grant；
- inspect effective authorization；

都属于 AgentRun-scoped command/query。HTTP 形态如有需要也只能位于 `/agent-runs/{run_id}/...`，不能恢复全局 `/permission-grants` 产品面。

Grant 不进入：

- AgentFrame；
- Business Agent Surface；
- ContextFrame；
- Capability artifact；
- VFS access source；
- Project Agent config；
- Shared Library；
- Hook snapshot；
- Driver config。

## 9. Permission 清理范围

### 9.1 独立 Grant 系统

删除：

- `agentdash-domain::permission`；
- `agentdash-application::permission`；
- `PermissionGrantRepository` 与 Postgres implementation；
- `permission_grants` table、indexes、reset/delete SQL；
- PermissionGrant API/contracts/frontend；
- approve/reject/revoke 断裂调用；
- policy engine、scope escalation、TTL sweep。

### 9.2 Surface/Capability/VFS

删除：

- `PermissionGrantApplied/Revoked` RuntimeSurfaceUpdateRequest；
- permission RuntimeSurfaceKind；
- Business Surface active grants query；
- PermissionGrant capability artifact；
- `RuntimeVfsAccessSource::PermissionGrant`；
- grant rules 向 VFS policy 的 merge。

### 9.3 permission_policy 与 Hook

删除 Project Agent `permission_policy` 及其：

- domain/config/contracts；
- Shared Library publish/install/override；
- Relay/Executor/vendor config；
- Hook snapshot/context；
- supervised approval preset/Rhai；
- frontend editor/view/context display。

通用 Hook rewrite/block/context/effect 与 RuntimeInteraction 能力保持。

## 10. 良好框架与过度设计的边界

### 10.1 良好框架

一个框架值得保留，需要满足：

1. 有当前或已确认的 producer；
2. 有当前或已确认的 consumer；
3. owner 唯一；
4. 解决明确正确性不变量；
5. 删除后有可写出的失败测试；
6. 对外契约窄，内部复杂度不外泄。

本设计中的合理框架：

- Surface revision/digest/CAS；
- Runtime operation journal/recovery；
- RuntimeInteraction durable wait/response；
- Tool Broker exactly-once；
- AgentRun permission facade；
- LifecycleRun 同行 typed document。

### 10.2 过度设计

出现以下情况即应删除或推迟：

- 为没有当前数据的未来状态建立独立 aggregate/table/repository/API/UI；
- 同一事实被投影到 Project config、Hook、Surface、VFS、Runtime 与 Driver；
- 为展示标签建立全局正确性前置条件；
- seam 的 production implementation 只是调用另一个已有 admission；
- pending approval 与长期 Grant 共享状态机；
- 低层模块直接读取上层聚合以“方便验证”；
- DTO 复制 vendor schema，却没有 AgentDash-owned 语义；
- 删除模块后没有任何当前行为或测试失败。

## 11. 验证设计

### Surface

- 无 phase 的 Canvas/VFS/MCP/Skill delta 可编译并 adopt；
- 有 phase 的 Workflow delta 保留 node metadata；
- replay frame IDs/digest 稳定；
- deterministic preflight failure 不推进 active current；
- Driver apply failure保留 accepted operation 并可恢复。

### Permission

- production permission facade 对所有当前请求返回 `Allowed`；
- Tool Broker、Driver Host 各调用同一 injected facade；
- no Grant repository read；
- no permission interaction on allow-all；
- synthetic `PendingApproval` test 能证明 RuntimeInteraction 链仍可工作；
- canonical approval contract 不引用 Codex generated DTO；
- capability/VFS/credential/binding tests 继续独立通过。

### Dependency

- `rg`/architecture test 阻止 Runtime、Integration、Surface、API 依赖 `LifecycleRunRepository` 读取 Grant；
- 只允许 AgentRun implementation 与 persistence adapter 出现未来 Grant ownership 描述；
- production code 无 PermissionGrant/table/policy/surface contribution 残留。
