# Workflow/VFS/Relay 模块边界拆分 Implement

## Order

1. 选择一个 area，不混合多个大模块。
2. 阅读对应 spec：
   - Workflow: `.trellis/spec/backend/workflow/architecture.md`
   - VFS: `.trellis/spec/backend/vfs/architecture.md`
   - Relay: `.trellis/spec/cross-layer/desktop-local-runtime.md` 和 docs relay 文档
3. 创建子模块文件。
4. 移动类型/helper，保留 `mod.rs` re-export。
5. 运行 format/check。
6. 更新 spec 或 review note。

## Validation

```powershell
cargo fmt
cargo check -p agentdash-domain -p agentdash-application -p agentdash-relay -p agentdash-agent
```

按实际改动缩小 package 集合。

## Review Focus

- diff 应主要是 move/re-export。
- serde tag、ts-rs/export、public enum variant 名称保持不变。
- 拆分后搜索旧路径/旧模块名，确保没有孤立引用。

## Progress

- 已确认第一批不直接混合 Workflow、VFS、Relay、Agent loop 四个区域。
- 当前任务输出是分阶段提交顺序；后续直接在当前架构收敛父任务下按批次提交机械拆分。
- Stage 1 已拆分 Workflow validation 边界：
  - 新增 `crates/agentdash-domain/src/workflow/validation.rs` 承载 Workflow contract、Lifecycle DAG、Activity lifecycle 校验逻辑。
  - `workflow/value_objects.rs` 保留可序列化 value types、capability directive reduction、binding helper。
  - `workflow/mod.rs` 继续 re-export public validation API，调用方不需要改公开路径。
  - 已验证 `cargo test -p agentdash-domain workflow::value_objects`、`cargo test -p agentdash-domain workflow::validation`、`cargo check -p agentdash-domain -p agentdash-application`。
- Stage 2 已拆分 VFS tools 共享边界：
  - 新增 `crates/agentdash-application/src/vfs/tools/common.rs` 承载 `SharedRuntimeVfs`、URI resolution、tool text result helper。
  - 新增 `crates/agentdash-application/src/vfs/tools/mounts.rs` 承载 `mounts_list` discovery tool。
  - `vfs/tools/fs.rs` 保留 file/search/patch/shell tools，并 re-export 旧路径上的 shared types，保持当前调用面稳定。
  - 已验证 `cargo check -p agentdash-application`、`cargo check -p agentdash-api`。
- Stage 3 已拆分 Relay protocol 握手 payload：
  - 新增 `crates/agentdash-relay/src/protocol/handshake.rs` 承载 register、ping/pong、capabilities、agent/mcp info payload。
  - `protocol.rs` 保留顶层 `RelayMessage` 信封和公共 `pub use`，不改变 serde tag 或 wire format。
  - 已验证 `cargo check -p agentdash-relay -p agentdash-api -p agentdash-local`。
