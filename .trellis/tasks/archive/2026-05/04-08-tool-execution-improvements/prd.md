# 工具执行层改进

> 状态：in_progress
> 参考：`references/pi-mono/packages/coding-agent/src/core/tools/`
> 2026-05-18 收口结论：本 task 只保留“并行工具调用下同一文件写入串行化”作为当前缺口；`cwd` / tool operations profile 相关目标已被后续 VFS、Capability 与 MCP relay 架构吸收，不在本 task 内重复建模。

## 背景

对比 pi-coding-agent 的工具执行层，原始 PRD 记录了三个待改进点。结合当前项目状态重新检查后，结论如下：

### 问题一：并发写入无序列化保护（仍需处理）

我们的 agent loop 支持并行 tool 调用（`ExecutionMode::Parallel`）。当多个工具调用同时写同一文件时，存在 race condition。pi-coding-agent 通过 `FileMutationQueue` 将写操作串行化。

当前代码中 `ToolExecutionMode::Parallel` 会对 prepared tool call `tokio::spawn` 并发执行。VFS 写工具当前稳定形态是 `fs_apply_patch`，同一 patch 可能触达多个 mount/path。因此本 task 需要在 VFS 写工具执行阶段按写目标 key 串行化：

- 不影响读工具和 shell 工具并发。
- 不同文件写入仍可并发。
- 同一个 `fs_apply_patch` 触达多个文件时，需要同时持有所有目标 key 的锁，避免 A 改 `a,b` 与 B 改 `b,c` 交错。
- 锁顺序必须稳定，避免死锁。

### 问题二：Shell Tool 的 spawn 级 cwd/env 注入（已被当前架构吸收）

当前 `shell_exec` 已经是 VFS runtime tool，schema 显式暴露 `cwd`，并通过 `ExecRequest { mount_id, cwd, command, timeout_ms, streaming_call_id }` 进入 `MountProvider::exec`。执行目录不再依赖 MCP tool schema 是否提供参数，也不应在 MCP adapter 层新增独立 `ShellExecInterceptor`。

`environment_variables` 已作为 `ExecutionSessionFrame` 字段存在，并由各 connector 消费。是否把 env 透传到 VFS `ExecRequest` 属于后续“执行环境投影完整性”问题，不是本 task 原先设想的 MCP spawn 拦截问题。

### 问题三：Pluggable Tool Operations 无正式 Contract（已由 VFS/Capability/MCP 方向替代）

pi-coding-agent 为每个工具抽象了 `Operations` trait（如 `BashOperations`、`ReadFileOperations`），可被替换为 SSH 远程实现。

当前项目已经以会话级 `Vfs`、`MountProvider`、`CapabilityState`、direct MCP / relay MCP discovery 表达工具后端差异：

- 文件读写与 shell 执行由 mount provider 能力决定。
- 工具可见性由 `CapabilityResolver` / `CapabilityState` / `tool_policy` 决定。
- 外部工具实现由 `SessionMcpServer`、direct MCP 和 relay MCP 注入。

因此本 task 不再新增 `ToolOperationsProfile` / address-space profile。若未来要做“同一 session 内按 mount/profile 切换 shell backend”的增强，应作为 VFS/MountProvider 能力扩展单独建 task。

---

## 收口设计

### 1. VFS Mutation Queue

对 `fs_apply_patch` 按 patch 中声明的文件目标做互斥串行化。

```rust
// agentdash-application/src/vfs/mutation_queue.rs
pub(crate) struct MutationQueue {
    locks: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl MutationQueue {
    pub async fn with_locks<F, T>(&self, keys: Vec<String>, f: F) -> T
    where F: Future<Output = T>;
}
```

**集成点**：`FsApplyPatchTool::execute()` 在调用 `RelayVfsService::apply_patch_multi()` 前，通过工具自身持有的共享 `MutationQueue` 包装真正的 VFS 写入。

**边界原则**：

- Agent Loop 不识别任何具体工具名、patch grammar 或 VFS mount 语义。
- `AgentTool` 基础 trait 不新增 mutation contract，避免把当前工具执行策略过早平台化。
- `fs_apply_patch` 的写目标解析留在 VFS 工具层，因为这里拥有 schema、VFS snapshot、默认 mount 和 patch parser。
- 后续若多个写工具需要共享同一套策略，再在 application/tool assembly 层抽出 wrapper；本 task 不提前引入该抽象。

**写目标 key 提取**：

- `FsApplyPatchTool` 使用现有 `parse_patch_text()` 解析 `patch`，而不是重复手写 grammar。
- args 的 `mount` 或 VFS default mount 作为无 mount 前缀路径的默认 namespace。
- patch 内带 `mount_id://path` 前缀时，key 使用显式 mount。
- 无法提取目标时不加锁，保持工具执行错误由原工具返回。

**注意**：读操作不需要锁；不同文件的写操作仍然并发；只有同一文件的并发写才串行化。

---

## 验收标准

1. 并行 tool call 下，两个 `fs_apply_patch` 写同一个目标文件时不会并发进入工具实现。
2. 两个 `fs_apply_patch` 写不同目标文件时仍可并发进入工具实现。
3. 单个 patch 涉及多个目标文件时，任一目标重叠都会串行化。
4. 非写工具不受 `MutationQueue` 影响，Agent Loop 并行执行语义保持不变。
5. 相关单元测试覆盖同文件串行、不同文件并行和 move target key。

## 非目标

- 不新增 `ShellExecInterceptor`。
- 不新增 `ToolOperationsProfile`。
- 不为已淘汰的 `write_file` / `fs_write` 名称补兼容逻辑。
- 不改变 Agent Loop、VFS/MCP/Capability 的跨层契约。

## 实施顺序

1. 更新 task 结论，标明过时项与保留项。
2. 在 `agentdash-application::vfs` 实现 `MutationQueue`。
3. 将 `FsApplyPatchTool` 写入路径接入队列。
4. 增加 VFS tool / queue 单元测试。
5. 运行格式化和相关 Rust 测试。
