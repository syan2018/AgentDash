# Agent 工具系统（内置工具注册与执行）

## 1. 背景与目标

`agentdash-agent` 已具备 Pi Agent 风格的核心循环（agent_loop、steering、follow-up），也定义了 `AgentTool` / `DynAgentTool` trait。但当前没有任何具体工具实现——Pi Agent 能"思考"但无法"操作"。

本任务的目标是为 Pi Agent 实现一组核心内置工具，使其具备实际的编程辅助能力：
1. 文件系统操作（读取、写入、列目录）
2. Shell 命令执行（受控的命令行操作）
3. 代码搜索（grep/ripgrep 风格的内容搜索）
4. 工具注册框架（支持动态添加自定义工具）

## 2. 当前约束

1. `AgentTool` trait 已定义：`name()`, `description()`, `parameters_schema()`, `call()`
2. `DynAgentTool` 作为 type-erased 包装，支持动态分发
3. `AgentContext` 持有 `workspace_path`，是工具执行的工作目录
4. `agent_loop` 中已有工具调用逻辑：解析 LLM 返回的 tool_use → 匹配工具 → 调用 → 构造 tool_result
5. 工具的参数 schema 需要是 JSON Schema 格式（供 LLM function-calling 使用）

## 3. Goals / Non-Goals

### Goals

- **G1**: 实现 `ReadFileTool`——读取文件内容（支持行范围）
- **G2**: 实现 `WriteFileTool`——写入/创建文件
- **G3**: 实现 `ListDirectoryTool`——列出目录内容（支持递归/深度限制）
- **G4**: 实现 `ShellTool`——执行 shell 命令（带超时和输出限制）
- **G5**: 实现 `SearchTool`——在工作空间中搜索文件内容（正则/文本）
- **G6**: 建立 `ToolRegistry`——集中管理工具注册、查找、schema 生成
- **G7**: 工具安全边界——所有文件操作限制在 `workspace_path` 内（路径遍历防护）

### Non-Goals

- 不实现 Web 浏览/HTTP 请求工具（后续扩展）
- 不实现代码执行沙箱（Shell 工具即可满足基本需求）
- 不实现 MCP 工具桥接（已有 `agentdash-mcp` 负责）
- 不做工具权限的细粒度控制（用 ShellTool 的命令白名单/黑名单替代）

## 4. ADR-lite（核心决策）

### 决策 A：工具实现放在 agentdash-agent crate 内

工具与 Agent Runtime 同属一个 crate（`agentdash-agent/src/tools/`），因为：
- 工具需要访问 `AgentContext`（workspace_path 等）
- 避免跨 crate 的复杂依赖
- 未来可拆为独立 crate，但当前不必要

### 决策 B：路径安全使用 canonicalize + starts_with

所有文件操作先 `canonicalize` 解析符号链接和相对路径，再检查是否以 `workspace_path` 开头。
拒绝任何越界访问。

### 决策 C：ShellTool 使用 tokio::process::Command

异步执行，带超时（默认 30s，可配）。stdout/stderr 合并返回，输出截断到 max_output_chars（默认 50000）。
不做命令黑名单——Agent 对其工作空间有完全控制权。

### 决策 D：工具 Schema 使用 schemars 自动生成

工具参数定义为 `#[derive(JsonSchema)]` 的 struct，运行时自动生成 JSON Schema。
避免手写 schema 的维护负担。

## 5. Signatures

### 5.1 ToolRegistry

```rust
// agentdash-agent/src/tools/registry.rs
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn DynAgentTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, tool: impl AgentTool + 'static);
    pub fn register_dyn(&mut self, tool: Arc<dyn DynAgentTool>);
    pub fn get(&self, name: &str) -> Option<Arc<dyn DynAgentTool>>;
    pub fn list(&self) -> Vec<ToolInfo>;
    pub fn all_schemas(&self) -> Vec<serde_json::Value>;
    
    /// 注册所有内置工具
    pub fn with_builtins(workspace_path: PathBuf) -> Self;
}

pub struct ToolInfo {
    pub name: String,
    pub description: String,
}
```

### 5.2 ReadFileTool

```rust
// agentdash-agent/src/tools/read_file.rs
#[derive(Deserialize, JsonSchema)]
pub struct ReadFileParams {
    /// 要读取的文件路径（相对于工作空间根目录）
    pub path: String,
    /// 起始行号（1-based，可选）
    pub start_line: Option<usize>,
    /// 结束行号（1-based，包含，可选）
    pub end_line: Option<usize>,
}

pub struct ReadFileTool {
    workspace_path: PathBuf,
}
// 返回：文件内容（带行号前缀）
```

### 5.3 WriteFileTool

```rust
// agentdash-agent/src/tools/write_file.rs
#[derive(Deserialize, JsonSchema)]
pub struct WriteFileParams {
    /// 要写入的文件路径（相对于工作空间根目录）
    pub path: String,
    /// 文件内容
    pub content: String,
    /// 是否追加模式（默认 false，覆盖写入）
    pub append: Option<bool>,
}

pub struct WriteFileTool {
    workspace_path: PathBuf,
}
```

### 5.4 ListDirectoryTool

```rust
// agentdash-agent/src/tools/list_directory.rs
#[derive(Deserialize, JsonSchema)]
pub struct ListDirectoryParams {
    /// 目录路径（相对于工作空间根目录，默认 "."）
    pub path: Option<String>,
    /// 是否递归（默认 false）
    pub recursive: Option<bool>,
    /// 递归最大深度（默认 3）
    pub max_depth: Option<usize>,
}

pub struct ListDirectoryTool {
    workspace_path: PathBuf,
}
```

### 5.5 ShellTool

```rust
// agentdash-agent/src/tools/shell.rs
#[derive(Deserialize, JsonSchema)]
pub struct ShellParams {
    /// 要执行的命令
    pub command: String,
    /// 工作目录（相对于工作空间根目录，默认为根目录）
    pub cwd: Option<String>,
    /// 超时秒数（默认 30）
    pub timeout_secs: Option<u64>,
}

pub struct ShellTool {
    workspace_path: PathBuf,
    default_timeout: Duration,
    max_output_chars: usize,
}
// 返回：{ exit_code, stdout, stderr, timed_out }
```

### 5.6 SearchTool

```rust
// agentdash-agent/src/tools/search.rs
#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// 搜索模式（文本或正则）
    pub pattern: String,
    /// 搜索目录（相对于工作空间根目录，默认 "."）
    pub path: Option<String>,
    /// 文件名 glob 过滤（如 "*.rs"）
    pub include: Option<String>,
    /// 排除 glob（如 "node_modules"）
    pub exclude: Option<Vec<String>>,
    /// 最大返回结果数（默认 50）
    pub max_results: Option<usize>,
    /// 是否使用正则表达式（默认 false）
    pub regex: Option<bool>,
}

pub struct SearchTool {
    workspace_path: PathBuf,
}
// 返回：匹配列表 [{ file, line, content }]
```

## 6. Contracts

### 6.1 路径安全契约

所有接受 `path` 参数的工具必须：
1. 将相对路径与 `workspace_path` 拼接
2. `canonicalize()` 后检查 `starts_with(workspace_path)`
3. 越界时返回 `ToolError::PathTraversal`

### 6.2 工具返回值契约

所有工具返回 `Result<String, AgentError>`：
- 成功：返回人类可读的文本（供 LLM 理解）
- 失败：返回错误描述（同样是文本，LLM 可据此调整策略）

### 6.3 与 PiAgentConnector 的集成契约

`PiAgentConnector::prompt()` 在创建 `Agent` 实例时：
1. 构建 `ToolRegistry::with_builtins(workspace_path)`
2. 将 registry 中的工具注入到 `Agent` 的工具列表
3. 工具调用事件通过 `AgentEvent::ToolCallStarted / ToolCallCompleted` 流出
4. PiAgentConnector 将这些事件转换为 ACP `ToolCall` notification

## 7. Validation & Error Matrix

| 场景 | 工具 | 错误类型 | 行为 |
|------|------|----------|------|
| 文件不存在 | ReadFile | 返回错误文本 | LLM 可重试 |
| 路径越界 | All file tools | PathTraversal | 返回明确拒绝信息 |
| 目录不存在 | ListDirectory | 返回错误文本 | LLM 可创建 |
| 命令超时 | Shell | TimedOut | 返回已收集的输出 + 超时标记 |
| 命令不存在 | Shell | 返回 exit_code!=0 | LLM 可调整命令 |
| 输出过长 | Shell/Search | 截断 | 附带截断提示 |
| 正则语法错误 | Search | 返回错误文本 | LLM 可修正 |

## 8. Good / Base / Bad Cases

### Good
- Agent 收到 "读取 src/main.rs" → 调用 ReadFile → 返回带行号的内容 → Agent 理解代码
- Agent 需要修复 bug → Shell("cargo test") → 看到错误 → ReadFile → WriteFile → Shell("cargo test") → 通过

### Base
- Agent 尝试读取不存在的文件 → 收到错误 → 尝试 ListDirectory 找到正确路径 → 成功读取

### Bad
- Agent 尝试 ReadFile("../../etc/passwd") → 路径安全检查拦截 → 返回 "路径越界"
- Shell 命令执行超过 30s → 超时中断 → 返回部分输出 + "命令已超时"

## 9. 验收标准

- [ ] `ToolRegistry` 支持注册和查找工具
- [ ] `ToolRegistry::with_builtins()` 注册所有 5 个内置工具
- [ ] `ReadFileTool` 支持全文和行范围读取，带行号
- [ ] `WriteFileTool` 支持创建和覆盖/追加
- [ ] `ListDirectoryTool` 支持递归和深度限制
- [ ] `ShellTool` 支持超时和输出截断
- [ ] `SearchTool` 支持文本和正则搜索
- [ ] 所有文件工具通过路径安全检查
- [ ] `PiAgentConnector` 在 prompt 时注入工具
- [ ] 工具调用事件正确转换为 ACP ToolCall notification
- [ ] 参数 schema 通过 schemars 自动生成

## 10. 模块结构

```
crates/agentdash-agent/src/
├── tools/
│   ├── mod.rs              # 模块导出 + ToolRegistry
│   ├── registry.rs         # ToolRegistry 实现
│   ├── safety.rs           # 路径安全检查工具函数
│   ├── read_file.rs        # ReadFileTool
│   ├── write_file.rs       # WriteFileTool
│   ├── list_directory.rs   # ListDirectoryTool
│   ├── shell.rs            # ShellTool
│   └── search.rs           # SearchTool
├── agent.rs
├── agent_loop.rs
├── bridge.rs
├── convert.rs
├── event_stream.rs
├── types.rs
└── lib.rs
```

## 11. 实施拆分（建议）

### Phase 1: 框架与安全（约 1.5h）
1. ToolRegistry 实现
2. 路径安全模块（safety.rs）
3. 添加 schemars 依赖

### Phase 2: 核心工具（约 3h）
4. ReadFileTool
5. WriteFileTool
6. ListDirectoryTool
7. ShellTool
8. SearchTool

### Phase 3: 集成（约 1.5h）
9. `ToolRegistry::with_builtins()` 
10. PiAgentConnector 中注入工具
11. 工具事件 → ACP ToolCall 转换验证

## 12. 依赖

- `schemars`：JSON Schema 自动生成
- `tokio::process`：异步命令执行（已有）
- `walkdir` 或 `ignore`：目录遍历
- `grep-regex` 或手动 `regex` + `walkdir`：搜索
