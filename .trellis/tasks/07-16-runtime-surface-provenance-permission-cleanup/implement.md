# Runtime Surface 来源与 Permission 清理实施计划

## 1. 执行约束

- 全程使用 Codex 主会话内联实施与检查，不派发 subagent。
- 实现前加载 `trellis-before-dev`，读取涉及 package 的 spec 与 checklist。
- 不恢复旧 Permission API，不保留兼容字段、双轨 contract 或 fallback。
- 本任务只清理当前实现并保留 AgentRun 权限验证入口，不实现 LifecycleRun Grant。
- 工作区若出现并行修改，只处理本任务文件，不覆盖或整理他人改动。

## 2. Phase 0：建立特征化测试与依赖基线

### 0.1 Surface 回归测试

先补能复现当前错误的测试：

- Canvas visibility update 产生 normalized delta、无 `transition_phase_node`；
- `RuntimeSurfacePresentationPlan::for_adoption` 当前返回 `MissingTransitionPhase`；
- application path 证明新 Frame 已写入而 adoption 失败。

目标不是保留错误断言，而是先固定回归链，再随实现改为成功断言。

重点文件：

- `crates/agentdash-agent-runtime/src/context_projection/artifact.rs`
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs`
- `crates/agentdash-application-agentrun/tests/`

### 0.2 Permission 依赖清单

生成并保存本次清理基线：

```powershell
rg -n "PermissionGrant|permission_grant|PermissionGrantApplied|PermissionGrantRevoked" crates packages/app-web/src
rg -n "permission_policy|supervised_tool_approval|supervised_tool_gate" crates packages/app-web/src
rg -n "RuntimeVfsAccessSource::PermissionGrant|permission_grant\\(\\)" crates
rg -n "temporary_permission_approval|CommandApproval|FileChangeApproval|PermissionApproval" crates
```

将命中按以下 owner 分类：

- independent Grant system；
- Surface/Capability/VFS contribution；
- Project Agent/config/hook propagation；
- RuntimeInteraction canonical contract；
- Tool Broker/Driver validation seam；
- docs/spec/tests。

## 3. Phase A：修正 Surface provenance 与发布边界

### A1. phase 改为可选 presentation metadata

- 删除 `RuntimeSurfacePresentationPlanError::MissingTransitionPhase`。
- `project_live_surface_transition` 与各 dimension renderer 接收可选 phase metadata。
- 有 phase 使用 Workflow transition 表达；无 phase 使用通用 Runtime Surface Update。
- frame identity 只使用 operation/source frame/revision/ordinal。
- 保留 `transition_phase_node: Option<String>` 作为 metadata，不参与 adoption 合法性。

重点文件：

- `crates/agentdash-agent-runtime/src/context_projection/artifact.rs`
- `crates/agentdash-agent-runtime/src/context_projection/live.rs`
- `crates/agentdash-agent-runtime/src/context_projection/dimension/`
- `crates/agentdash-agent-runtime/src/surface.rs`
- `crates/agentdash-application-agentrun/src/agent_run/context_sources.rs`

### A2. deterministic preflight 前移

- 审查 Canvas 与通用 Runtime Surface update 在 `frame_repo.create` 前能够得到的全部 compile inputs。
- 在新 Frame 对 current read model 可见前完成：
  - hook plan validation；
  - Business Surface compile；
  - normalized delta；
  - presentation plan compile。
- 持久化 candidate 后再提交 canonical `SurfaceAdopt`。
- current surface query 必须读取 Runtime 已采用的 source frame，不以最高 revision 代替 adopted state。
- durable accept 后 Driver apply failure继续进入既有 recovery。

重点文件：

- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs`
- `crates/agentdash-application-agentrun/src/agent_run/business_frame_surface_query.rs`
- `crates/agentdash-api/src/bootstrap/agent_runtime_surface.rs`
- Runtime surface adoption persistence/composition files

### A3. Surface 验证

最小测试：

```powershell
cargo test -p agentdash-agent-runtime context_projection
cargo test -p agentdash-agent-runtime runtime_interface
cargo test -p agentdash-application-agentrun runtime_surface_update
```

必须覆盖：

- Canvas create/expose/present/bind-data 无 phase 成功；
- Workflow phase metadata 保留；
- replay identity 稳定；
- compile failure 不推进 current；
- accepted operation 的 Driver failure 可恢复。

## 4. Phase B：建立唯一 AgentRun 权限门面

### B1. 定义 dependency-light contract

定义 protocol-neutral：

- `AgentRunPermissionRequest`；
- `AgentRunPermissionDecision::{Allowed, Denied, PendingApproval}`；
- `AgentRunPermissionError`；
- async permission facade/port。

contract 只携带：

- AgentRun/runtime execution coordinates；
- normalized action/resource；
- binding/idempotency coordinate。

不携带 Grant model、repository、AgentFrame、Surface、vendor DTO、policy revision 或 UI 字段。

具体 contract crate 在实现前按依赖图确定，要求：

- `agentdash-agent-runtime` 与 Integration 可依赖；
- 不引入对 `agentdash-application-agentrun` 的反向依赖；
- production implementation owner 仍位于 AgentRun application。

### B2. 实现 current allow-all AgentRun facade

- 在 `agentdash-application-agentrun` 实现唯一 production facade。
- 当前固定返回 `Allowed`。
- 不注入 `LifecycleRunRepository`、Grant store 或 policy engine。
- 在 composition root 注入 Tool Broker 与 Driver Host 所需边界。

### B3. Tool Broker 接入

- 删除 production permission implementation 对 capability admission 的重复调用。
- 在 capability、Hook rewrite、VFS normalization 后调用 AgentRun permission facade。
- `Allowed` 继续 credential/executor。
- `Denied` 产生 typed permission terminal。
- `PendingApproval` 关联 canonical RuntimeInteraction 并暂停 side effect。
- 当前 production 测试断言只出现 `Allowed`。

### B4. Driver/Adapter 接入

- Codex command/file/permission request 先映射为 AgentDash-owned permission request。
- 统一调用 injected AgentRun facade。
- 当前 `Allowed` 立即回复 vendor。
- 保留 `Denied/PendingApproval` contract 与测试，不建立 Grant 实现。
- Native/Remote/Relay 不复制第二套 permission policy。

### B5. 门面验证

- Tool Broker 与 Driver Host 使用同一个 production facade instance。
- allow-all 请求不创建 RuntimeInteraction。
- capability/VFS deny仍在 permission 之前阻止无效动作。
- permission allow之后才允许 credential resolution 与 side effect。
- architecture test/rg 确认 Runtime/Integration 不依赖 `LifecycleRunRepository`。

## 5. Phase C：保留并清理 RuntimeInteraction approval

### C1. canonical owned DTO

- 保留 command/file/permission approval interaction family。
- 将 generated Codex approval DTO 替换为 AgentDash-owned request。
- 只保留当前 Driver/Tool Broker 真正能够提供、未来 UI/decision 必需的字段。
- Adapter 负责 vendor ↔ canonical translation。

### C2. 删除临时伪造路径

- 删除 `temporary_permission_approval` 及空 permissions/cwd/zero timestamp 拼装。
- 检查 Hook、Tool Broker、test-support 中所有 temporary approval helper。
- 测试必须显式构造 canonical request，不能依赖 vendor fixture。

### C3. 动态审批能力验证

使用 synthetic non-production facade 返回 `PendingApproval`，验证：

- RuntimeInteraction durable create；
- AgentRun runtime facade 可以 resolve；
- duplicate resolve 幂等；
- lost/recovery 语义不退化；
- 当前 allow-all production 不走该路径。

本 Phase 不创建 Grant，也不实现用户审批 UI。

## 6. Phase D：删除独立 PermissionGrant 系统

### D1. Domain/Application

删除：

- `crates/agentdash-domain/src/permission/` 及导出；
- `crates/agentdash-application/src/permission/` 及导出；
- Grant aggregate/status/scope/policy/escalation/TTL；
- `PermissionGrantRepository`；
- PermissionGrant compiler 与 effect service。

### D2. Surface/Capability/VFS

删除：

- `PermissionGrantApplied/Revoked`；
- permission RuntimeSurfaceKind；
- Business Surface active grants query 与 dependency；
- PermissionGrant capability artifact source；
- `RuntimeVfsAccessSource::PermissionGrant`；
- grant-specific VFS rules merge 与 fixtures。

Grant 变化不再创建 AgentFrame revision或触发 Surface adoption。

### D3. Infrastructure 与 migration

删除：

- `PostgresPermissionGrantRepository`；
- repository set/bootstrap/AppState wiring；
- AgentRun delete/reset 中的 permission grant SQL；
- migration assertion 中的旧表清理要求。

新增下一版本 migration：

```sql
DROP TABLE permission_grants;
```

使用 `DROP TABLE` 自然删除其 indexes/constraints。历史 migration 不修改。

本任务不向 `lifecycle_runs` 添加 grants 字段。

### D4. API/contracts/frontend

删除：

- PermissionGrant route/module；
- Permission contracts 与 TS generation registration；
- frontend service/types/card/index；
- approve/reject/revoke 断裂调用；
- Companion/session UI 中宣称 PermissionGrant 审批的文案。

当前不新增替代 Grant API/UI。

### D5. 删除 permission_policy 横向传播

从以下链路删除 `permission_policy`：

- AgentConfig/ProjectAgent domain；
- contracts/generated TS；
- conversation/runtime options；
- Shared Library publish/install/override；
- Relay/Executor/Codex config；
- Hook snapshot/Rhai context；
- supervised approval preset/global rule；
- frontend preset editor/project view/context overview。

保留通用 Hook Continue/Rewrite/Block/Context/Effect。

## 7. Phase E：Spec 收敛

更新：

- `.trellis/spec/backend/agent-runtime-context.md`
  - phase 是可选 presentation metadata；
  - adoption 由 revision/digest/CAS/operation identity 证明。
- `.trellis/spec/backend/agent-runtime-surface-tool-broker.md`
  - Tool Broker 调用 AgentRun permission facade；
  - 不读取 Grant 或合并 Grant VFS。
- `.trellis/spec/backend/agent-runtime-agentrun-facade.md`
  - AgentRun 是 permission/approval/未来 Grant 的唯一产品暴露面。
- `.trellis/spec/backend/agent-runtime-kernel.md`
  - RuntimeInteraction approval 是 durable wait/response，不是 Grant。
- `.trellis/spec/backend/agent-runtime-codex-adapter.md`
  - vendor approval 映射为 owned contract。
- `.trellis/spec/backend/workflow/architecture.md`
  - 未来 Grant 属于 `LifecycleRun` 同行 typed document；
  - 通过同一聚合仓储持久化，只由 AgentRun application 使用。
- `.trellis/spec/backend/permission/architecture.md`
  - 重写为 AgentRun permission facade、当前 allow-all 与未来 ownership。

删除不再成立的：

- `.trellis/spec/backend/permission/policy-engine.md`
- `.trellis/spec/backend/permission/grant-lifecycle.md`

文档只记录最终架构与理由，不保留旧实现流水账。

## 8. 全局验证

### 8.1 静态搜索

```powershell
rg -n "PermissionGrant|permission_grant|PermissionGrantApplied|PermissionGrantRevoked" crates packages/app-web/src
rg -n "permission_policy|supervised_tool_approval|supervised_tool_gate" crates packages/app-web/src
rg -n "RuntimeVfsAccessSource::PermissionGrant|permission_grant\\(\\)" crates
rg -n "temporary_permission_approval" crates
rg -n "changed surface adoption requires workflow transition phase provenance" crates
```

允许命中只限最终 spec、migration 历史与必要测试说明；生产实现不得残留。

### 8.2 定向测试

```powershell
cargo fmt --all -- --check
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-integration-codex
cargo test -p agentdash-application-vfs
cargo test -p agentdash-api
pnpm --filter @agentdash/app-web test
pnpm --filter @agentdash/app-web typecheck
pnpm run contracts:check
pnpm run migration:guard
```

### 8.3 最终门禁

```powershell
cargo check --workspace --all-targets
```

Cargo 被 rust-analyzer 占锁时遵循项目说明等待，不终止并行 IDE/Agent 进程。

## 9. 完成审计

逐项确认：

- Canvas 与 Workflow 两类 Surface adoption 均通过；
- no-phase presentation 使用通用文案；
- 未采用 Frame 不成为 current；
- production permission 统一经过 AgentRun facade；
- Tool Broker/Runtime/Integration 无 LifecycleRun Grant 直连；
- allow-all 不创建 pending interaction；
- synthetic pending approval 仍能完成 RuntimeInteraction 链；
- canonical approval contract 无 vendor DTO；
- PermissionGrant table/repository/API/UI/surface contribution 已删除；
- `permission_policy` 与 supervised permission hook 已删除；
- 未向 `lifecycle_runs` 添加 Grant 字段；
- 未创建未来 Grant CRUD/API/UI；
- 每个保留 seam 均能回答 producer、consumer、owner、invariant、failure test 五问。

## 10. 风险与实施顺序

- Surface current/adopted 语义风险最高，先补测试再调整写入顺序。
- AgentRun permission facade 应先落地，再删除旧 Tool Broker/Driver permission paths，避免出现无判定入口的中间状态。
- RuntimeInteraction approval contract 先 canonicalize，再删除 vendor DTO 与 temporary helper。
- Grant 删除顺序采用 consumer → wiring → implementation → migration，保证每一步编译错误都指向待清理依赖。
- `permission_policy` 横跨生成 contracts 与前端，删除后立即运行 codegen/typecheck。
- 本任务不通过提前实现 LifecycleRun Grant 来“验证未来设计”；未来实现另建任务。
