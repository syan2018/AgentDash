# execution_hooks 迁移到 application 层

## Goal

将 `agentdash-api/src/execution_hooks/` 整体迁移到 `agentdash-application/src/hooks/`。
Hook 评估是独立的执行策略关注点，不嵌套在 workflow 子领域下。

## Background

`execution_hooks/` 包含 `AppExecutionHookProvider` 的完整实现（实现了
`agentdash_connector_contract::hooks::ExecutionHookProvider` trait）。
这是纯粹的**应用层用例**——读取 domain repos、workflow projection、lifecycle 状态
来决定 hook 策略。

它不依赖任何 api 层类型——零 `use crate::` 引用。

### 当前文件结构

```
api/src/execution_hooks/
├── mod.rs              (1920 行) — AppExecutionHookProvider + 核心评估逻辑
├── rules.rs            (578 行)  — Hook 规则引擎
└── snapshot_helpers.rs (299 行)  — Hook 快照构建辅助
```

### 依赖分析

| 依赖 | 来源 | application 是否已有 |
|------|------|---------------------|
| `agentdash_domain::*` repos | domain | ✅ |
| `agentdash_executor::ExecutionHookProvider` 等 | executor (via connector-contract) | ✅ |
| `agentdash_application::workflow::*` | application 自身 | ✅ |

**零外部阻塞，可直接搬运。**

## Target Location

```
application/src/hooks/
├── mod.rs              — AppExecutionHookProvider + 核心评估逻辑
├── rules.rs            — Hook 规则引擎
└── snapshot_helpers.rs — Hook 快照构建辅助
```

**选择 `application/hooks/` 而非 `application/workflow/hooks/` 的理由**：
- Hook 评估是独立的执行策略关注点
- 虽然当前 hook 规则大量引用 workflow/lifecycle 状态，但 hook 本身的概念
  （before_tool / after_turn / session_start 等触发点）不属于 workflow 子领域
- 未来可能有非 workflow 相关的 hook 规则（如安全策略、资源配额等）

## Requirements

1. 将 `api/src/execution_hooks/` 三个文件移至 `application/src/hooks/`
2. 更新 `application/src/lib.rs` 声明 `pub mod hooks;`
3. 更新 `api` 侧的 import（从 `crate::execution_hooks` 改为 `agentdash_application::hooks`）
4. `api/src/execution_hooks/` 变为薄 re-export 或直接删除
5. 测试跟着代码一起搬运

## Acceptance Criteria

- [ ] `cargo check -p agentdash-application` 通过
- [ ] `cargo check -p agentdash-api` 通过
- [ ] `cargo test -p agentdash-application` 包含迁移后的 execution hooks 测试
- [ ] `cargo test -p agentdash-api --lib` 通过（原有的 hooks 测试不退化）
- [ ] `api/src/execution_hooks/` 行数 < 10（仅 re-export）或已删除

## Technical Notes

- `app_state.rs` 构造 `AppExecutionHookProvider` 时传入 repos，迁移后只需改 import 路径
- `api/src/execution_hooks/` 可保留为 `pub use agentdash_application::hooks::*;`，
  避免修改所有下游引用（渐进式迁移）
- 该模块与 address_space_access 拆分无冲突，可独立执行

## Dependency Chain

```
前置：03-27-agent-tool-dependency-decoupling (SPI 下沉，优先级 #1)
并行：03-27-migrate-address-space-services-to-application (无冲突)
后续：减少 api crate 职责负担
```
