# W7 Product / Protocol generated activation evidence

## 用途

S4 的 Product target 直接消费
`agentdash_agent_runtime_contract::managed_projection::ManagedRuntimeProjectionSchema`，
而正式 TypeScript bindings 与 canonical JSON Schema 要在 S5 production route、generator
root 和 consumer 同时切换时生成。这里保存同一个 Rust root 机械派生的 task-local schema，
让 S5 在修改 canonical generator 前可以审阅完整类型闭包和 fixture drift。

`manifest.json` 固定 schema 与前端 canonical fixture 的 SHA-256。API target test 会重新运行
`schemars::schema_for!(ManagedRuntimeProjectionSchema)`，验证 schema 内容、hash 和 typed
fixture 反序列化结果，因此该目录不是手写的平行合同。

## 产物

- `managed-runtime-projection.schema.json`：canonical Rust projection root 的确定性 JSON
  Schema。
- `manifest.json`：generator identity、canonical root、schema hash、前端 fixture hash 与复现
  命令。
- `packages/app-web/src/features/session/model/fixtures/managedRuntimeProjection.json`：由
  canonical Rust snapshot/change 类型序列化，并由 API target test typed 反序列化校验。

## 复现与检查

```powershell
$env:AGENTDASH_UPDATE_W7_ACTIVATION_ARTIFACTS = "1"
cargo test -p agentdash-api --test agent_runtime_target_projection task_local_generated_artifacts_match_canonical_schema_and_frontend_fixture -- --exact
Remove-Item Env:AGENTDASH_UPDATE_W7_ACTIVATION_ARTIFACTS
cargo test -p agentdash-api --test agent_runtime_target_projection
```

S5 将 `ManagedRuntimeProjectionSchema` 纳入正式 Runtime Contract generator root 后，使用
workspace 的 `contracts:generate` / `contracts:check` 原子更新 canonical bindings、schema
和 Product caller。

生产 caller、repository transaction、共享热点和硬切顺序冻结在相邻目录
`activation/w7-product-cutover/`。这里的 schema 只作为 canonical generator input，不在
Product component 中修改正式 generated root。
