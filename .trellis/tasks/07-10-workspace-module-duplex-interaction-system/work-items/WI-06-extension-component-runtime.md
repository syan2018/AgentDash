# WI-06 Extension Component Runtime

Status: planned

Depends On: WI-02、WI-04、Channel task WI-01

## Scope

- `ui_components[]` descriptor/toolchain/contracts。
- isolated iframe host、CSP、MessageChannel、schema/sizing/rate limits。
- component props/state projection 与 typed event schema-validation/payload-pass-through binding；目标为 exact platform command 或即时 Operation/OperationScript。
- Extension Component + canonical Operation contribution；无 Extension reducer。
- exact artifact resolution/pinning、disable/upgrade structured readiness。
- digest-addressed artifact retention 与 runtime binding 引用清理合同。

## Exit Criteria

- component 不接触 Project/session/backend/workspace authority。
- component 不直接持有通用 invoke 权限。
- component/definition contract version 固定，既有 instance 不受新 handler/component 语义影响。
- existing instance 固定 exact artifact digest；upgrade 只影响 new definition/new instance。
- 被 existing instance 引用的 artifact 在引用/retention 允许前保持可寻址。
- 不存在自动 rebind 或通用 state migration engine。

## Validation

- manifest/toolchain/contract checks。
- CSP/origin/MessagePort/schema/rate/size tests。
- browser component composition smoke。
