# WI-01 RuntimeGateway Operation Core

Status: done

Depends On: WI-00

## Scope

- provider-qualified Operation descriptor/catalog。
- principal/scope/origin/placement/trace envelope。
- actor-specific surface resolver、placement resolver 与 common execution core。
- schema/effect/replay-policy/capability admission、cancellation、trace、scoped result ref、output validation。

## Exit Criteria

- direct invoke 与 OperationScript nested invoke 共享唯一 execution core。
- Agent/User/Workflow/Extension service principal 不共享或伪造 identity。
- 客户端不能提交 backend/session/workspace authority。
- result ref 继承 principal/scope/capability/TTL，不能作为 bearer token。
- 旧 RuntimeAction descriptor/admission 重复路径有明确删除清单。

## Validation

- RuntimeGateway unit/property tests。
- capability revocation/TOCTOU/placement/readiness tests。
- affected Rust package check/clippy。

## Implementation Evidence

- `b9067fe5` 建立 provider-qualified descriptor/catalog、server-owned resolved envelope 与唯一
  `OperationExecutionCore`，覆盖 schema、effect/replay、capability、readiness/placement、deadline/cancel、
  TOCTOU 二次 admission、audit、output validation 与 scoped result ref。
- `dbea13f6` / `e3b562f8` 将 stable principal/scope/origin refs 与 effect/replay policy 下沉到
  `agentdash-domain::operation`，供 RuntimeGateway、Interaction 与 OperationScript 共享。
- Setup provider 与所有 Setup direct caller 已迁入 `OperationGateway::invoke(OperationInvocationCommand)`；
  host command 不携带 authority revision 或 placement，Gateway 每次通过 injected access port 重验
  Project、Workspace、Backend 与 ProjectBackendAccess facts，并生成授权 digest。
- 旧 `setup_actions.rs` 已删除；Setup input/output schema、server placement、缺失 backend scope 与 capability
  revocation tests 由 `setup_operations.rs` 固定。

## Remaining Host-Adapter Residual

下列旧 RuntimeAction 路径只服务尚未完成的 WI-02 host authority adapter，不能在本项中以 Session 改名包装：

- `runtime_gateway/{gateway,provider,types,error,session_actions,tool_adapter}.rs`：AgentRun MCP 与 Agent tool
  仍需从 AgentFrame/current delivery evidence 解析 principal/scope/origin。
- `runtime_gateway/extension_actions.rs`、`api/routes/extension_runtime.rs`：Extension panel/service 仍需 exact
  installation principal 与 Project/workspace placement resolver。
- `api/routes/canvases.rs`、`workspace-module/src/workspace_module/*`：Canvas/UserWorkshop standalone bridge
  尚未提供，不应继续把 RuntimeSession 当 authority。

WI-02 完成上述 resolver 后回到本项删除这些 residual，并将状态推进到 checking。
