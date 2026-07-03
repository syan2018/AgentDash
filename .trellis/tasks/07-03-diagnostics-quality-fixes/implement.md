# 实施计划

## 0. 开始前

1. 阅读 `.trellis/spec/backend/diagnostics-guidelines.md`。
2. 阅读 `.trellis/spec/guides/cross-layer-thinking-guide.md`，确认错误诊断不改变业务合同。
3. 阅读本任务 `research/diagnostics-quality-inventory.md`，按优先级选择修复分片。

## 1. 审计复核

运行以下扫描，确认当前基线：

```powershell
rg -n "diag!\\((Warn|Error)" crates
rg -n "tracing::(info|warn|error|debug|trace)!" crates
rg -n "operation\\s*=|DiagnosticErrorContext|diag_error!" crates
```

产出：

- 更新 research 或实现 notes，标出本次准备修复的 P0/P1 调用点。
- 对已经合格的 `diag!(Warn/Error)` 标注保留理由，例如无错误对象、只是状态告警。

## 2. 分片实现

优先顺序：

1. P0：`diag!(Error, ..., "{error}")`、`diag!(Error, ..., error = %e)` 但没有 `diag_error!` 的调用点。
2. P1：`diag!(Warn, ...)` 持有错误对象但缺少 `operation` / `stage` / 关联字段。
3. P2：message 含动态上下文但未结构化字段化的关键路径。

实现要求：

- 引入 `use agentdash_diagnostics::{diag_error, DiagnosticErrorContext, ...};`。
- 在错误分支附近创建 `let context = DiagnosticErrorContext::new("operation", "stage");`，避免跨长代码块复用含糊 context。
- 保留原有日志级别，除非现有级别明显不符合规范。
- 对可恢复降级继续用 `Warn`；对用户请求失败、关键后台任务失败、运行不可继续用 `Error`。
- 不改变 `return Err(...)`、`map_err(...)`、HTTP status、事件 payload 或前端可见错误文案。

## 3. 验证

每个实现分片至少运行：

```powershell
cargo fmt --check
cargo check -p <changed-crate>
```

整体收尾运行：

```powershell
rg -n "tracing::(info|warn|error|debug|trace)!" crates
rg -n "diag!\\((Warn|Error).*\\{.*error|diag!\\((Warn|Error).*error\\s*=" crates
cargo clippy --workspace --all-targets -- -D warnings
```

如果 clippy 或全 workspace check 因既有无关问题失败，记录：

- 命令。
- 失败 crate / 文件。
- 为什么与本任务无关。
- 已经跑过的替代检查。

## 4. Review 重点

- `operation` 是否稳定、可搜索、不是自然语言句子。
- `stage` 是否足够具体，能区分同一 operation 内不同失败点。
- `error` 是否通过 `diag_error!` 注入，而不是只在 message 里格式化。
- 字段是否足够排障，同时没有敏感信息。
- 没有引入新裸 `tracing::*` 事件宏。
