# WI-01 RuntimeGateway Operation Core

Status: planned

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
