# WI-03 OperationScript

Status: implementing

Depends On: WI-01

## Scope

- `rhai_v1` + host API V1、input、allowed operation manifest 与 bounded limits。
- Application `OperationScriptEngine` port / executor，Infrastructure `RhaiScriptRuntime` adapter。
- execution-scoped evaluator factory + bounded worker admission/async host-call bridge，不阻塞 Tokio core worker。
- execution-scoped `ops.invoke` 隐式等待 / `ops.invoke_all` structured concurrency。
- preflight plan token 绑定 source/input/manifest/limits/principal/scope/version/expiry；recursive-script rejection。
- root cancellation/progress hook、nested trace、partial/outcome-unknown evidence 与 scoped result ref。
- Agent、Canvas/UserWorkshop 和 Workflow callers；Canvas 只保存/提交 source，脚本由服务端执行。

## Exit Criteria

- 不建立 script asset、execution aggregate、background job 或跨调用 REPL state。
- 每个 nested Operation 重新经过 canonical execution core 的 operation/capability/schema/limits admission。
- caller cancellation、timeout、调用/并行/输出上限与 structured error 可观察。
- cancellation/deadline 能跨 blocking evaluator 与 async nested invocation 传播。
- `max_concurrent_scripts` 能阻止 worker pool exhaustion；纯 Rhai loop 也能响应取消。
- Rhai adapter 可被未来 sandbox adapter 替换而不改变外部请求和 execution port。

## Validation

- compile/JSON bridge/evaluator factory/`ops` host surface/manifest/token digest property tests。
- recursive rejection、worker exhaustion、CPU/host-call cancel、timeout/limit/parallel/partial outcome/scoped result tests。
- Agent、Canvas 与 Workflow executor parity tests。

## Implementation Evidence

- `30590c8b feat(operation-script): 建立异步脚本执行合同与 Rhai 沙箱`
  - 完成 HMAC preflight binding、bounded blocking worker、纯 Rhai 取消与 worker admission。
- `dd853978 feat(operation-script): 接入结构化 Operation 组合调用`
  - `ops.invoke` / buffered `ops.invoke_all` 经 `GatewayOperationScriptExecutor` 重入 canonical OperationGateway。
  - 每次 run 使用独立 execution id；失败结果保留 ordered call evidence、partial 与 outcome-unknown。
  - AST cache 按 entry/source bytes 有界淘汰；大结果使用 scoped ref，并在读取时重新校验当前 principal/scope/capability/TTL。
- Focused checks:
  - `cargo check -p agentdash-application-runtime-gateway -p agentdash-infrastructure`
  - `cargo test -p agentdash-infrastructure operation_script -- --nocapture`（11 passed）

## Remaining Integration

- Agent、Canvas/UserWorkshop 与 Workflow caller wiring/parity 由各自工作项接入同一 execution port 后闭合。
