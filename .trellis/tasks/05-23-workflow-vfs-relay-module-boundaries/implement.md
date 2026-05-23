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
- 已创建四个后续子任务，分别承接 workflow value objects、VFS、Relay protocol、Agent loop 的目录级拆分。
- 当前总控任务的输出是拆分顺序和任务边界；具体代码移动在子任务中逐个执行。
