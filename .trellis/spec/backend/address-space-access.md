# Scenario: 统一 Address Space / Runtime Access Layer（跨层契约）

### 1. Scope / Trigger
- **Trigger**: 当功能同时涉及云端上下文注入、本机文件访问、Agent 运行时工具调用、多 workspace 挂载或非物理 workspace（KM / Snapshot / 资源库）时，必须使用统一的 Address Space 抽象，而不是继续新增独立访问链路。
- **影响面**: `task_agent_context`、`declared sources`、relay `workspace_files` / `tool.*`、本机 `ToolExecutor`、PiAgent runtime tools、未来 KM warp。

---

### 2. Signatures（目标接口 / 类型 / 工具面）

#### 2.1 核心对象

```rust
struct Mount {
    id: String,
    provider: String,
    root_ref: String,
    capabilities: CapSet,
    default_write: bool,
    display_name: String,
}

struct ResourceRef {
    mount_id: String,
    path: String,
}
```

#### 2.2 Provider 抽象

```rust
#[async_trait]
trait AddressSpaceProvider {
    async fn read(&self, target: &ResourceRef, opts: ReadOpts) -> Result<ReadResult, AccessError>;
    async fn write(&self, target: &ResourceRef, content: WriteContent) -> Result<WriteResult, AccessError>;
    async fn list(&self, target: &ResourceRef, opts: ListOpts) -> Result<ListResult, AccessError>;
    async fn search(&self, query: SearchQuery) -> Result<SearchResult, AccessError>;
    async fn stat(&self, target: &ResourceRef) -> Result<StatResult, AccessError>;
    async fn exec(&self, req: ExecRequest) -> Result<ExecResult, AccessError>;
}
```

#### 2.2.1 命名注意（当前代码现状）

- 当前代码里的 `agentdash-injection::AddressSpaceProvider` 仅用于暴露 address space descriptor，服务 `/api/address-spaces` 能力发现。
- 它还不是本规范这里的统一读写 provider，不承担 `read / write / list / search / exec`。
- 后续落地时必须显式决定是：
  - 扩展现有 descriptor provider
  - 另抽一层真正的 runtime provider
  - 或重命名其中一层
- 禁止在实现中默认把这两个同名概念视为同一层抽象，否则很容易造成“接口已存在”的误判。

#### 2.3 运行时工具面

必须优先收敛为稳定的小工具集合：

- `mounts.list`
- `fs.read`
- `fs.write`
- `fs.list`
- `fs.search`
- `shell.exec`

公共定位参数：

```json
{ "mount": "main", "path": "crates/agentdash-api/src/app_state.rs" }
```

执行参数：

```json
{ "mount": "main", "cwd": ".", "command": "cargo test -p agentdash-api" }
```

---

### 3. Contracts（字段、能力、边界）

#### 3.1 资源定位契约
- Agent 和上层用例**不应直接感知** `backend_id + absolute path`。
- 统一定位方式为 `mount + relative path`。
- `mount` 是会话级挂载 ID，例如 `main / spec / km / snapshot`。
- `path` 必须是相对 mount 根的路径。

#### 3.2 Session Mount Table 契约
- 每个 Task / Story / Session 启动时必须生成一份 mount table。
- mount table 至少包含：
  - `id`
  - `provider`
  - `root_ref`
  - `capabilities`
  - `default_write`
- `main` mount 代表当前 Task 绑定的执行 workspace。
- 对于只读空间（如 `spec` / `snapshot`），必须显式声明不支持 `write` / `exec`。

#### 3.3 Provider 能力契约

| 能力 | 说明 | 物理 workspace | KM / Snapshot |
|------|------|----------------|---------------|
| `read` | 读取文本/资源内容 | 必须支持 | 必须支持 |
| `write` | 写入资源 | 可支持 | 按 provider 决定 |
| `list` | 列出目录或条目 | 必须支持 | 必须支持 |
| `search` | 搜索内容/路径 | 推荐支持 | 推荐支持 |
| `stat` | 查询元信息 | 推荐支持 | 推荐支持 |
| `exec` | 执行命令 | 仅物理可执行 mount 支持 | 默认不支持 |

#### 3.4 relay 契约
- relay 是访问本机 provider 的 transport，不是 mount 模型本身。
- 上层逻辑不应直接在 `context resolver` 或 runtime tool 中拼接 `RelayMessage`。
- 物理 workspace 的 cloud 访问可由 `relay_fs_provider` 实现，内部再使用：
  - `command.workspace_files.*`
  - `command.tool.*`

#### 3.5 context provider / runtime tool 一致性契约
- 声明式来源解析与运行时工具访问必须共享同一套 provider 底座。
- `File / ProjectSnapshot` 不应长期保留专属实现路径。
- 如果某资源可被 context 注入读取，也应能在 runtime tool 中以相同 mount/path 访问。

#### 3.6 非物理 workspace warp 契约
- KM / Snapshot / 资源库应呈现为“受限 VFS”。
- 暂不承诺完整 POSIX 语义。
- 默认不支持：
  - `shell.exec`
  - symlink
  - chmod
  - file lock
  - watch
  - 原子 rename

---

### 4. Validation & Error Matrix

| 条件 | 预期行为 | 错误语义 |
|------|----------|----------|
| `mount` 不存在 | 拒绝执行 | `NotFound` |
| `path` 为绝对路径 | 拒绝执行 | `InvalidPath` |
| `path` 含 `..` 越界 | 拒绝执行 | `PathEscapesMount` |
| mount 不支持该能力 | 拒绝执行 | `CapabilityDenied` |
| 目标 backend 不在线 | 拒绝执行 | `BackendOffline` |
| provider 不可用 | 拒绝执行 | `ProviderUnavailable` |
| relay 超时 | 标记为 transport 失败 | `Timeout` |
| KM / Snapshot 请求 `exec` | 直接拒绝 | `CapabilityDenied` |

补充规则：
- `shell.exec` 只允许在声明了 `exec` 能力的 mount 上执行。
- `fs.write` 默认只允许写入 `default_write = true` 的 mount，除非上层显式授权。

---

### 5. Good / Base / Bad Cases

#### Good
- Story 上下文把默认 workspace 挂为 `main`，把规范仓挂为 `spec`，Agent 同时读取两个 mount：

```json
{ "tool": "fs.read", "mount": "main", "path": "Cargo.toml" }
{ "tool": "fs.read", "mount": "spec", "path": "backend/address-space-access.md" }
```

#### Base
- 当前阶段 provider 内部仍可暂时复用现有 relay `workspace_files` 协议，只要上层接口已统一到 provider。

#### Bad
- 直接把 `backend_id` 和绝对路径暴露给 Agent：

```json
{
  "backend_id": "backend-a",
  "workspace_root": "F:\\Projects\\AgentDash",
  "path": "crates/agentdash-api/src/app_state.rs"
}
```

问题：
- Agent 需要理解部署细节
- 多 mount / 非物理空间无法统一
- 上下文注入和 runtime tool 无法共享同一定位模型

---

### 6. Tests Required（断言点）

#### Provider 层
- 给定 `mount=main, path=foo/bar.rs`，能正确路由到目标 provider。
- `path` 为绝对路径或含 `..` 时必须被拒绝。
- provider 能力矩阵正确生效：无 `exec` 的 mount 不允许执行命令。

#### relay / local provider
- `Task.workspace_id -> Workspace.backend_id` 路由正确。
- `workspace_root` 真正影响本机执行根目录，而非仅记录日志。
- 本机路径写入不会逃逸出 mount 根目录。

#### context resolver
- `File / ProjectSnapshot` 来源通过 provider 成功读取。
- provider 失败时 required source 直接报错，optional source 产生 warning。

#### runtime tools
- `mounts.list` 返回当前会话可访问的 mount 清单。
- `fs.read/write/list/search` 使用统一的 `mount + path` 参数模型。

---

### 7. Wrong vs Correct

#### Wrong：为每种访问场景单独长一套协议
```text
context source -> command.workspace_files.*
PiAgent runtime -> BuiltinToolset::for_workspace(...)
future KM -> km_tool.*
future snapshot -> snapshot_tool.*
```

问题：
- 四套定位模型
- 权限和错误语义无法统一
- 多 workspace / 非物理空间难以复用

#### Correct：先统一到底层 Address Space，再暴露稳定工具面
```text
declared source
runtime tool
frontend read-only browse
        ↓
Address Space Provider
        ↓
relay_fs / local_fs / km / snapshot
```

优势：
- 定位模型统一
- 安全边界统一
- transport 与领域抽象解耦

---

### 8. First Implementation Slice
- 先修复本机 prompt 执行真正 honor `workspace_root`
- 先补强本机路径边界
- 再抽 `AddressSpaceProvider`
- 再让 declared source 优先接入 provider
- 最后推动 PiAgent runtime tool 迁移

---

### 9. Design Decision

#### 决策：采用“统一 provider + 小工具集合”，不采用“万能工具”

**Context**:
- 当前系统已存在 `workspace_files`、`tool.*`、内置工具三套访问路径
- 后续还需要支持多 workspace 和非物理 workspace

**Decision**:
- 底层统一为 Address Space Provider
- 上层统一为 `mount + relative path`
- Agent 工具保持小而稳定

**Why**:
- 更利于模型稳定调用
- 更利于权限控制和错误矩阵定义
- 更适合与 context provider 共享实现
