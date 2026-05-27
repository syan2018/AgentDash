# Local Extension Runtime 模块目录化 Design

## Layout

采用最小目录化：

```text
crates/agentdash-local/src/
  extensions/
    mod.rs
    artifact_cache.rs
    host.rs
```

`extensions/mod.rs` 负责对子模块做 re-export，`lib.rs` 再从 `extensions` re-export crate 外稳定入口。这样后续如果 runner/protocol/permissions 继续膨胀，可以在 `extensions/host/` 下二次拆分，而本任务只做低风险移动。

## Non-Goals

- 不拆 TS runner 大字符串。
- 不重排 handlers 目录。
- 不改变权限、artifact download 或 relay activation 行为。
