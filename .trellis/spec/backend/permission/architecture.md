# Permission Architecture

## Scenario: AgentRun Permission Boundary

### 1. Scope / Trigger

当 Tool Broker 执行工具、Integration 上报动态审批请求，或未来为 AgentRun 增加长期 Grant 时，适用本合同。权限事实属于运行实例，不属于 Project Agent 配置、Business Surface、VFS、Hook 或 Driver。

### 2. Signatures

当前唯一入口：

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
    PendingApproval { interaction_id: String, reason: String },
}
```

当前 production implementation 为 `AllowAllAgentRunPermissionFacade`，不读取仓储或配置。

未来长期 Grant 的持久化签名属于 `LifecycleRunRepository` 聚合读写；它是 `LifecycleRun` 同行 typed document，不建立独立 Grant repository 或 table。外部读写由 AgentRun application facade 提供。

### 3. Contracts

- `AgentRunPermissionRequest` 只含 AgentDash-owned 坐标：`run_id`、`agent_id`、`runtime_session_id`、`turn_id`、`item_id`、`capability_key`、`tool_name`。
- Tool availability 由 Capability/VFS 等各自边界判断；permission facade 不重复投影这些事实。
- 当前权限事实固定为 `Allowed`，因此不产生 permission-owned pending interaction。
- Driver 的动态审批请求转换为 `RuntimePermissionApprovalRequest` 后进入 canonical RuntimeInteraction；vendor DTO 只存在于 Integration adapter。
- credential broker 只解析已声明 credential slot，不参与 permission decision。
- 未来 Grant 的创建、撤销、审批协调、read model 与 RuntimeInteraction resolve 都由 AgentRun 层提供；Tool Broker、Driver、Integration、API 不直接读取 `LifecycleRunRepository`。
- 动态批准顺序为：AgentRun 持久化 Grant fact，再 resolve RuntimeInteraction；恢复执行前再次调用 permission facade。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| --- | --- |
| 当前 production authorize | `Allowed`，零 repository I/O |
| facade 返回 `Denied` | Tool side effect 前终止，返回 reason |
| facade 返回 `PendingApproval` 且 interaction ID 非法 | typed execution error，不伪造 interaction |
| Integration 收到 vendor permission approval | 严格校验 vendor payload，再转换为 canonical request |
| 非 AgentRun 层尝试读取未来 Grant | 依赖方向检查失败 |
| 未来 Grant 写入成功但 interaction resolve 失败 | Grant fact 保留，resolve 可幂等重试 |

### 5. Good / Base / Bad Cases

- Good：Tool Broker 完成 binding/capability/VFS 检查后，将稳定坐标交给 AgentRun permission facade；当前得到 `Allowed` 并执行。
- Good：Codex permission request 在 integration 层转成 `RuntimePermissionApprovalRequest`，Runtime 与 UI 不依赖 Codex generated type。
- Base：未来没有 Grant 字段时，allow-all facade 保持无状态。
- Bad：为 Grant 建独立 aggregate/table/repository，再向 Surface、VFS、Hook、API 和 UI 投影同一事实。
- Bad：permission facade 再次查询 CapabilityState 来重复决定工具是否可用。

### 6. Tests Required

- `AllowAllAgentRunPermissionFacade` 单测：任意 protocol-neutral request 均返回 `Allowed`。
- Tool Broker 测试：`Allowed / Denied / PendingApproval` 分支均在 tool side effect 前处理。
- Integration mapping 测试：vendor permission request 的 request identity 与 canonical payload 坐标保持一致。
- 依赖搜索：生产代码无独立 Grant repository/table/API/UI/surface contribution。
- migration 测试：schema 只保留 LifecycleRun 与 RuntimeInteraction 所需的持久化结构。

### 7. Wrong vs Correct

#### Wrong

```rust
tool_broker.load_grants(run_id).await?;
vfs.merge_grant_rules(grants);
surface.add_permission_contributions(grants);
```

同一事实被多个子系统解释，生命周期与依赖方向失去单一 owner。

#### Correct

```rust
let decision = agent_run_permission.authorize(request).await?;
```

AgentRun 是唯一权限入口；长期事实与 `LifecycleRun` 同生命周期，动态等待由 RuntimeInteraction 承载。

## Framework Boundary

良好框架必须同时具备当前 producer、consumer、owner、正确性不变量和失败测试。只有“未来也许会用”而没有当前调用链的 abstraction 不进入 production。

- 保留：AgentRun permission facade，因为 Tool Broker 当前需要稳定调用边界，未来 Grant 也必须保持同一依赖方向。
- 保留：RuntimeInteraction approval family，因为 Driver 当前会产生动态审批。
- 暂不实现：LifecycleRun Grant typed field、CRUD、policy engine、TTL、scope escalation 和 UI；这些需要真实产品语义后再设计。
- 扩展方式：在 facade 内增加 LifecycleRun-backed decision，不改变 Tool Broker、Driver 或 Integration 的依赖。
