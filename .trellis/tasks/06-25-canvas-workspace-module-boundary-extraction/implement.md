# Implement Plan: Canvas Workspace Module 边界预抽取

## Steps

1. 新建 `crates/agentdash-canvas`。
2. 在 workspace `Cargo.toml` 添加 member 和 workspace dependency。
3. 在新 crate 实现 identity/helper/error/constants。
4. 将 `agentdash-application` 增加 `agentdash-canvas` dependency。
5. 将 `agentdash-application/src/canvas/identity.rs` 改为调用或 re-export 新 crate helper，并映射错误类型。
6. 更新 `workspace_module` 中硬编码的 Canvas operation/view/renderer/module prefix 常量。
7. 补充或迁移单元测试。
8. 运行验证命令。

## Validation

```powershell
cargo test -p agentdash-canvas
cargo test -p agentdash-application workspace_module
cargo test -p agentdash-application canvas
cargo check --workspace
```

## Risk Controls

- 不移动 Canvas entity/repository，避免基础 domain 依赖重排过大。
- 不移动 runtime snapshot/resource service，避免和 Canvas interaction MVP 混成一个行为改动。
- `agentdash-application/src/canvas/identity.rs` 可先保留 wrapper，减少跨模块 import churn。
- 如果 `cargo check --workspace` 暴露循环依赖，停止拆分并回到父任务调整 crate 边界。
