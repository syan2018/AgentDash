# 工具执行层改进

> 状态：planning
> 参考：`references/pi-mono/packages/coding-agent/src/core/tools/`

## 背景

对比 pi-coding-agent 的工具执行层，我们有以下三个待改进点：

### 问题一：并发写入无序列化保护

我们的 agent loop 支持并行 tool 调用（`ExecutionMode::Parallel`）。当多个工具调用同时写同一文件时，存在 race condition。pi-coding-agent 通过 `FileMutationQueue` 将写操作串行化。

### 问题二：Shell Tool 的 spawn 级 cwd/env 注入

我们的 `before_tool_call` 通过 `ToolCallDecision::Rewrite { args }` 改写 JSON 参数，可以覆盖 `command` 字段，但无法**在 JSON args schema 之外**注入 `cwd` 和 `env`——这取决于 shell tool 是否将这两个字段暴露为 args。

pi-coding-agent 的 `BashSpawnHook` 在 OS spawn 层面拦截，可以无条件修改 `cwd`/`env`，无需 tool schema 配合。

由于我们用 MCP 承载 shell tool，此问题需要在 MCP 适配层解决。

### 问题三：Pluggable Tool Operations 无正式 Contract

pi-coding-agent 为每个工具抽象了 `Operations` trait（如 `BashOperations`、`ReadFileOperations`），可被替换为 SSH 远程实现。

我们通过 MCP server 的可替换性实现了等效能力，但没有正式的 address-space 粒度的 pluggable operations contract，不同 address space 无法声明"使用不同的 shell tool 实现"。

---

## 设计

### 1. File Mutation Queue

对写操作（write_file / fs_write / fs_apply_patch）和编辑操作（write_file / canvas_start）按**文件路径**做互斥串行化。

```rust
// agentdash-agent/src/file_mutation_queue.rs
pub struct FileMutationQueue {
    locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
}

impl FileMutationQueue {
    pub async fn with_file_lock<F, T>(&self, path: &Path, f: F) -> T
    where F: Future<Output = T>;
}
```

**集成点**：`execute_prepared_tool_call()` 在判定工具为写操作（`ToolKind::Edit`）时，通过 `FileMutationQueue` 包装执行。

**注意**：读操作不需要锁；不同文件的写操作仍然并发；只有同一文件的并发写才串行化。

### 2. Shell Tool Spawn-Level cwd/env 注入

**目标**：plugin 或 hook 规则能够在 shell 执行前注入 `cwd` 和 `env`，而不依赖 tool 的 JSON schema 是否暴露这两个字段。

**方案**：在 MCP tool adapter (`pi_agent_mcp.rs`) 层新增 `ShellExecInterceptor` trait：

```rust
pub trait ShellExecInterceptor: Send + Sync {
    fn intercept(&self, ctx: ShellExecContext) -> ShellExecContext;
}

pub struct ShellExecContext {
    pub command: String,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
}
```

`McpToolAdapter` 在执行 `shell_exec` 类工具前，先经过所有注册的 `ShellExecInterceptor`。

`ShellExecInterceptor` 通过 Plugin API 注册（类似 `SourceResolver`）。

**与 BeforeTool hook 的关系**：`before_tool_call` 操作 JSON args，适合修改 `command` 内容；`ShellExecInterceptor` 操作 spawn context，适合注入 `cwd`/`env`。两者互补。

### 3. Pluggable Tool Operations 正式化

为 address space 引入 `ToolOperationsProfile` 概念：每个 address space 可声明自己希望使用的工具实现（本地 MCP / SSH MCP / 自定义 MCP）。

```rust
// agentdash-domain
pub struct VfsToolProfile {
    pub shell_exec_mcp: Option<McpServerRef>,    // 覆盖默认 shell tool
    pub file_read_mcp: Option<McpServerRef>,     // 覆盖默认 read tool
    pub file_write_mcp: Option<McpServerRef>,    // 覆盖默认 write tool
}
```

`build_runtime_system_prompt()` 和 MCP tool injection 时，根据当前 address space 的 `ToolOperationsProfile` 选择正确的 MCP server。

**这是 pluggable ops 的 first-class 支持，目前只能靠手动配置不同 MCP server 实现。**

---

## 调研项

在实施前需要先确认：

1. **shell_exec 的 MCP server 是否已将 `cwd`/`env` 暴露为 tool args？** 如果是，则 `before_tool_call` Rewrite 已经足够，`ShellExecInterceptor` 可能不需要。需读 `pi_agent_mcp.rs` 和实际 MCP server schema。

2. **并行 tool 调用目前是否确实在同一 agent session 内真正并发？** 需确认 `ExecutionMode::Parallel` 的实际行为，以及是否已有其他序列化机制。

3. **address space 是否已有 tool 配置字段？** 避免重复建模。

## 实施顺序建议

1. 先做调研项（1-3），评估实际缺口
2. File Mutation Queue（改动小，收益确定）
3. ShellExecInterceptor（视调研结论决定是否需要）
4. ToolOperationsProfile（依赖 address space 模型，可能是独立 task）
