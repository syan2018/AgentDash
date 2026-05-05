---
name: 终端执行 — Relay 流式传递、交互式 PTY、前端实时展示
overview: |
  为 Session 中的 shell 工具建立完整的终端执行通道：
  1. 串行 shell_exec 输出流式推送（local→server→前端 SSE）
  2. 并行交互式 PTY 终端（用户可在工作面板实时操作）
  3. 会话级终端运行时缓存与生命周期管理
  4. 前端工具调用卡片实时输出 + 终端 Tab 完整实现
todos:
  - id: phase1-protocol
    content: "Phase 1 — 协议设计：定义所有 relay 消息类型、终端状态模型、session 缓存结构"
    status: pending
  - id: phase2-serial-streaming
    content: "Phase 2 — 串行流式输出：local streaming exec → relay 事件 → cloud CommandOutputDelta → 前端卡片"
    status: pending
  - id: phase3-pty-backend
    content: "Phase 3 — 交互式终端后端：local PTY Manager + cloud Session Terminal Cache + API"
    status: pending
  - id: phase4-frontend-terminal
    content: "Phase 4 — 前端终端面板：xterm.js + Terminal Store + SSE 连接 + 交互输入"
    status: pending
  - id: phase5-integration
    content: "Phase 5 — 集成联动：Promote 到终端面板、Agent 上下文推送、断连处理、用户干预"
    status: pending
isProject: false
---

# 终端执行 — Relay 流式传递、交互式 PTY、前端实时展示

## 1. 背景与目标

### 1.1 当前状态

当前 Pi Agent 的 `shell_exec` 执行流是**同步 request/response**：

```
Pi Agent (cloud)
  → ShellExecTool.execute()
    → RelayVfsService.exec()
      → relay_fs MountProvider.exec()
        → CommandToolShellExec (relay WS) → Local Backend
          → ToolExecutor::shell_exec()  // spawn 子进程, wait_with_output
        ← ResponseToolShellExec { exit_code, stdout, stderr }
      ← ExecResult
    ← AgentToolResult
  → item_completed(commandExecution) BackboneEvent
```

**关键问题**：
- 整个执行期间前端看到的是 `inProgress` 状态的空白工具卡片，直到命令结束才一次性显示全部输出
- 用户无法观察命令执行过程，无法判断是否卡死
- 无法取消长时间运行的单条命令（只能取消整个 turn）
- 无交互式终端能力

第三方执行器（Codex/Claude Code 等）通过 vibe-kanban 的 BackboneEvent 流已经推送 `command_output_delta`，前端 `useSessionStream` 已能处理增量输出并累积到 `accumulatedText`。

### 1.2 目标

| # | 目标 | 说明 |
|---|------|------|
| G1 | **串行命令实时输出** | Pi Agent shell_exec 执行过程中，输出实时流式推送到前端工具调用卡片 |
| G2 | **交互式终端** | 用户可在右侧工作面板打开完整 PTY 终端，通过 relay 通道操作远程 shell |
| G3 | **命令 → 终端 Promote** | Agent 执行的串行命令可被用户提升到终端面板中实时查看 |
| G4 | **终端运行时缓存** | Session 持有活跃终端列表作为运行时状态，状态转换产生生命周期事件 |
| G5 | **Agent 上下文感知** | 终端生命周期事件（创建/丢失/用户干预）推送为 platform BackboneEvent |

### 1.3 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 用户交互深度 | 完整 PTY（首期含 stdin + resize） | 并行终端必须像 IDE 终端一样完整可用 |
| 执行器覆盖 | 原生工具走 relay 协议，第三方走现有 event 流 | 统一前端消费接口，后端按来源分通道 |
| 持久化策略 | 全量持久化为 session events | 首期简单，后续可优化为摘要+截断 |
| Agent 通知粒度 | 仅生命周期事件 | 终端创建/丢失/超时/用户干预时推送 platform 事件 |
| 并行终端触发 | 双向：Agent background + 用户手动 + Promote | 最大灵活性 |

## 2. 架构设计

### 2.1 整体架构

```
┌──────────────── Frontend ──────────────────────┐
│                                                 │
│  ┌─ Session Chat ─┐   ┌── Workspace Panel ───┐ │
│  │ ToolCallCard    │   │  Terminal Tab        │ │
│  │ (streaming out) │──▶│  (xterm.js + input)  │ │
│  │ [Promote btn]   │   │  [PTY bidirectional] │ │
│  └────────┬────────┘   └─────────┬────────────┘ │
│           │ SSE events           │ SSE + REST    │
└───────────┼──────────────────────┼──────────────┘
            │                      │
┌───────────┴──────────────────────┴──────────────┐
│              Cloud Backend (Rust/Axum)           │
│                                                  │
│  SessionHub ─── SessionTerminalCache             │
│       │         (active terminals per session)   │
│       │                                          │
│  Pi Agent Loop ──▶ ShellExecTool                 │
│       │            ↓                             │
│  RelayVfsService ── exec_streaming()             │
│       │             emit CommandOutputDelta       │
│       │                                          │
│  Terminal API ── spawn / input / resize / kill    │
│       │           forward via relay protocol     │
│       │                                          │
│  BackboneEvent Pipeline ── SSE push              │
└──────────┬───────────────────────────────────────┘
           │ WebSocket (relay protocol)
┌──────────┴───────────────────────────────────────┐
│              Local Backend (agentdash-local)      │
│                                                   │
│  ToolExecutor::shell_exec_streaming()             │
│    → spawn process, pipe stdout/stderr            │
│    → stream output via event_tx                   │
│                                                   │
│  TerminalManager                                  │
│    → portable-pty PTY session management          │
│    → stdin forwarding, resize signals             │
│    → process lifecycle tracking                   │
│    → output buffering + event emission            │
└───────────────────────────────────────────────────┘
```

### 2.2 数据流

#### 2.2.1 串行 Shell Exec（流式输出）

```
Pi Agent call shell_exec
  → emit item_started(commandExecution)
  → RelayVfsService.exec_streaming(command, call_id, session_id)
    → relay: CommandToolShellExec (same as today)
    → local: ToolExecutor::shell_exec_streaming()
      → spawn process
      → for each output chunk:
          event_tx.send(EventToolShellOutput { call_id, delta, stream })
          → relay WS → cloud
          → cloud: emit CommandOutputDelta BackboneEvent
          → SSE → frontend → accumulatedText on ToolCallCard
      → process exits
    ← relay: ResponseToolShellExec { exit_code, stdout (summary), stderr }
  ← ExecResult
  → emit item_completed(commandExecution { aggregatedOutput })
```

关键变化：
- `CommandHandler::handle_tool_shell_exec()` 不再直接 await `ToolExecutor::shell_exec()`
- 而是调用新的 `shell_exec_streaming()` 方法，该方法边执行边通过 `event_tx` 推送输出
- 云端收到 `EventToolShellOutput` 后立即构造 `CommandOutputDelta` BackboneEvent 并通过 SessionHub emit

#### 2.2.2 交互式终端（PTY）

```
User opens Terminal Tab / Agent spawns background terminal
  → frontend: POST /api/sessions/{id}/terminals  { cwd, shell? }
    → cloud: SessionTerminalCache.create(terminal_id)
    → relay: CommandTerminalSpawn { terminal_id, mount_root_ref, cwd }
    → local: TerminalManager::spawn(terminal_id, cwd)
      → portable-pty: create PTY pair
      → start reading master → event_tx(EventTerminalOutput)
    ← relay: ResponseTerminalSpawn { terminal_id, process_id }
    → cloud: SessionTerminalCache.update(terminal_id, Running)
    → emit platform BackboneEvent: terminal_spawned

User types in xterm.js
  → frontend: POST /api/sessions/{id}/terminals/{tid}/input  { data }
    → relay: CommandTerminalInput { terminal_id, data }
    → local: TerminalManager::write_stdin(terminal_id, data)
    ← relay: ResponseTerminalInput (ack)

Terminal output arrives
  → local: TerminalManager read loop
    → event_tx(EventTerminalOutput { terminal_id, data })
    → relay WS → cloud
    → cloud: emit platform BackboneEvent: terminal_output
    → SSE → frontend → xterm.js.write(data)

Terminal exits
  → local: TerminalManager detects exit
    → event_tx(EventTerminalStateChanged { terminal_id, Exited(code) })
    → relay WS → cloud
    → cloud: SessionTerminalCache.update(terminal_id, Exited)
    → emit platform BackboneEvent: terminal_exited
```

### 2.3 终端状态模型

```rust
// ─── 终端实例 ───
pub struct TerminalInstance {
    pub id: String,              // UUID
    pub session_id: String,
    pub backend_id: String,
    pub mount_root_ref: String,
    pub cwd: String,
    pub shell: Option<String>,   // 用户指定的 shell 程序
    pub process_id: Option<u32>,
    pub state: TerminalState,
    pub created_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
    /// 关联的 tool call item ID（串行命令 promote 时设置）
    pub linked_item_id: Option<String>,
}

pub enum TerminalState {
    Starting,
    Running,
    Exited { exit_code: i32 },
    Lost,       // backend 断连导致丢失
    Killed,     // 用户主动终止
}
```

### 2.4 Session Terminal Cache

```rust
/// Session 级终端运行时缓存
/// 不持久化到数据库，仅在内存中维护
pub struct SessionTerminalCache {
    terminals: HashMap<String, TerminalInstance>,
}

impl SessionTerminalCache {
    pub fn create(&mut self, terminal: TerminalInstance);
    pub fn update_state(&mut self, terminal_id: &str, state: TerminalState);
    pub fn get(&self, terminal_id: &str) -> Option<&TerminalInstance>;
    pub fn list_active(&self) -> Vec<&TerminalInstance>;
    pub fn handle_backend_disconnect(&mut self, backend_id: &str) -> Vec<String>; // 返回丢失的 terminal_ids
}
```

## 3. Relay 协议扩展

### 3.1 新增消息类型

```rust
// ─── 串行 shell exec 流式输出（local → cloud）───
#[serde(rename = "event.tool.shell_output")]
EventToolShellOutput {
    id: String,
    payload: ToolShellOutputPayload,
}

pub struct ToolShellOutputPayload {
    pub call_id: String,
    pub delta: String,
    pub stream: ShellOutputStream,  // "stdout" | "stderr"
}

pub enum ShellOutputStream { Stdout, Stderr }

// ─── 交互式终端命令（cloud → local）───
#[serde(rename = "command.terminal.spawn")]
CommandTerminalSpawn { id: String, payload: TerminalSpawnPayload }

#[serde(rename = "command.terminal.input")]
CommandTerminalInput { id: String, payload: TerminalInputPayload }

#[serde(rename = "command.terminal.resize")]
CommandTerminalResize { id: String, payload: TerminalResizePayload }

#[serde(rename = "command.terminal.kill")]
CommandTerminalKill { id: String, payload: TerminalKillPayload }

// ─── 交互式终端响应（local → cloud）───
#[serde(rename = "response.terminal.spawn")]
ResponseTerminalSpawn { id, payload?, error? }

#[serde(rename = "response.terminal.input")]
ResponseTerminalInput { id, payload?, error? }

#[serde(rename = "response.terminal.resize")]
ResponseTerminalResize { id, payload?, error? }

#[serde(rename = "response.terminal.kill")]
ResponseTerminalKill { id, payload?, error? }

// ─── 交互式终端事件（local → cloud）───
#[serde(rename = "event.terminal.output")]
EventTerminalOutput { id: String, payload: TerminalOutputPayload }

#[serde(rename = "event.terminal.state_changed")]
EventTerminalStateChanged { id: String, payload: TerminalStateChangedPayload }
```

### 3.2 Payload 定义

```rust
pub struct TerminalSpawnPayload {
    pub terminal_id: String,
    pub session_id: String,
    pub mount_root_ref: String,
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

pub struct TerminalSpawnResponse {
    pub terminal_id: String,
    pub process_id: Option<u32>,
}

pub struct TerminalInputPayload {
    pub terminal_id: String,
    pub data: String,  // raw bytes base64 or UTF-8 text
}

pub struct TerminalResizePayload {
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

pub struct TerminalKillPayload {
    pub terminal_id: String,
    pub signal: Option<String>,  // "SIGTERM" | "SIGKILL", 默认 SIGTERM
}

pub struct TerminalOutputPayload {
    pub terminal_id: String,
    pub data: String,  // PTY 原始输出（含 ANSI 序列）
}

pub struct TerminalStateChangedPayload {
    pub terminal_id: String,
    pub state: String,  // "running" | "exited" | "lost" | "killed"
    pub exit_code: Option<i32>,
    pub message: Option<String>,
}
```

## 4. 前端设计

### 4.1 Terminal Zustand Store

```typescript
interface TerminalInfo {
  id: string;
  sessionId: string;
  state: 'starting' | 'running' | 'exited' | 'lost' | 'killed';
  exitCode?: number;
  linkedItemId?: string;  // 关联的 tool call item
  cwd: string;
  createdAt: number;
}

interface TerminalStore {
  terminals: Map<string, TerminalInfo>;
  // Actions
  addTerminal(info: TerminalInfo): void;
  updateState(id: string, state: TerminalInfo['state'], exitCode?: number): void;
  removeTerminal(id: string): void;
  getBySession(sessionId: string): TerminalInfo[];
}
```

### 4.2 专用命令执行卡片（CommandExecutionCard）

`commandExecution` 类型的 ThreadItem 从通用 `AcpToolCallCard` 中拆出，使用独立的 `CommandExecutionCard` 组件。
通用 ToolCallCard 继续服务 `fileChange` / `mcpToolCall` / `dynamicToolCall` 等类型。

**拆分理由**：
- 命令执行的输出是**流式终端文本**（含 ANSI 序列），渲染逻辑与静态 JSON 结果完全不同
- 需要 xterm.js 轻量渲染（或至少 monospace + ANSI→HTML），不适合用 `<pre>` 直接展示
- Promote 按钮、进程元信息（pid / 运行时长 / exit code）、用户干预操作是命令执行特有 UI

**CommandExecutionCard 布局**：
```
┌─────────────────────────────────────────────────┐
│ [RUN]  $ git status           ● 执行中  0:12    │  ← header: badge + command + status + elapsed
│         cwd: src/             pid: 4821          │  ← subheader: cwd + pid
├─────────────────────────────────────────────────┤
│ On branch main                                   │
│ Changes not staged for commit:                   │  ← 流式输出区域（monospace, ANSI 渲染）
│   modified:   src/index.ts                       │     - 自动滚底
│ ...                                              │     - 最大高度折叠 + 展开
├─────────────────────────────────────────────────┤
│ exit: 0                [⬈ 在终端中查看]           │  ← footer: exit code + promote 按钮
└─────────────────────────────────────────────────┘
```

**组件文件**：`frontend/src/features/session/ui/CommandExecutionCard.tsx`

**关键实现**：
- 从 `SessionEntry.tsx` 的 `item_started / item_completed` 分支中，按 `item.type === "commandExecution"` 路由到 `CommandExecutionCard`
- 流式输出从 `accumulatedText`（由 `command_output_delta` 累积）获取
- 使用 `ansi-to-html` 或 xterm.js headless addon 将 ANSI 转为可渲染 HTML
- 进度条状态：`inProgress`（脉冲动画）→ `completed`（绿色 exit 0）/ `failed`（红色 exit ≠ 0）
- Promote 按钮调用 `expandWorkspacePanel("terminal", uri)` 关联到终端 Tab

### 4.3 Terminal Tab 实现

替换当前 `terminal-tab.tsx` 占位符为完整实现：

```
Terminal Tab
├── xterm.js 终端渲染（含 ANSI 序列支持）
├── 连接状态指示器
├── Terminal 选择器（session 内多终端切换）
├── 工具栏：新建终端 / Kill / 清屏
└── 输入处理 → POST /api/.../input
```

### 4.4 前端 API 新增

```typescript
// Terminal Management
POST   /api/sessions/{id}/terminals           → 创建终端
GET    /api/sessions/{id}/terminals           → 列出活跃终端
POST   /api/sessions/{id}/terminals/{tid}/input  → 发送输入
POST   /api/sessions/{id}/terminals/{tid}/resize → 调整大小
DELETE /api/sessions/{id}/terminals/{tid}     → 终止终端

// Terminal Output Stream — 复用现有 SSE
// terminal_output 作为 platform BackboneEvent 推送
// 前端通过 session SSE 接收，按 terminal_id 路由到对应 xterm.js 实例
```

## 5. 实施计划

### Phase 1: 协议设计 & 基础类型

**范围**：定义所有新的 relay 消息类型、终端状态模型、前端类型

| 子任务 | 文件 | 说明 |
|--------|------|------|
| P1.1 | `agentdash-relay/src/protocol.rs` | 新增 `EventToolShellOutput` + 全部 Terminal relay 消息类型 |
| P1.2 | `agentdash-relay/src/protocol.rs` | 新增所有 Payload 结构体 |
| P1.3 | `agentdash-protocol/src/backbone.rs` | 无需修改，`CommandOutputDelta` 已存在 |
| P1.4 | `agentdash-protocol/src/platform.rs` | 新增 terminal 相关 PlatformEvent kind |
| P1.5 | `frontend/src/types/terminal.ts` | 前端终端类型定义 |
| P1.6 | `frontend/src/generated/backbone-protocol.ts` | ts-rs 重新生成（如有 backbone 变更） |

**验收**：所有类型编译通过，relay 消息可序列化/反序列化

### Phase 2: 串行流式 Shell 输出

**范围**：Pi Agent shell_exec 执行中实时推送输出到前端

| 子任务 | 文件 | 说明 |
|--------|------|------|
| P2.1 | `agentdash-local/src/tool_executor.rs` | 新增 `shell_exec_streaming()` — 逐行读取 stdout/stderr 并通过回调推送 |
| P2.2 | `agentdash-local/src/command_handler.rs` | `handle_tool_shell_exec()` 改用 streaming 版本，边执行边通过 `event_tx` 推送 `EventToolShellOutput` |
| P2.3 | `agentdash-api/src/relay/ws_handler.rs` | 处理 `EventToolShellOutput`：构造 `CommandOutputDelta` BackboneEvent，通过 SessionHub emit |
| P2.4 | `agentdash-api/src/relay/ws_handler.rs` | 需要在 ws_handler 中建立 call_id → session_id 的映射，以便将输出路由到正确的 session |
| P2.5 | 前端 | 验证 `useSessionStream` 已正确处理 `command_output_delta` 并更新工具调用卡片 |
| P2.6 | `CommandExecutionCard.tsx` | 新建专用命令执行卡片：流式输出渲染、ANSI 支持、进程元信息 |
| P2.7 | `SessionEntry.tsx` | commandExecution 路由到 CommandExecutionCard，其余类型保持 AcpToolCallCard |

**关键设计**：
- `shell_exec_streaming()` 使用 `tokio::io::BufReader` 逐行读取子进程的 `stdout` 和 `stderr`
- 读到的每行通过 `event_tx.send()` 推送为 `EventToolShellOutput`
- 进程退出后发送最终 `ResponseToolShellExec`（保持向后兼容）
- 云端需要维护 `call_id → (session_id, item_id)` 的映射才能正确构造 BackboneEnvelope

**call_id 路由方案**：
- 在 `relay_fs` MountProvider 发送 `CommandToolShellExec` 时，将 `call_id` 与当前 session context 关联
- 注册到 `BackendRegistry` 的一个 `pending_shell_calls: HashMap<String, ShellCallContext>` 中
- 云端收到 `EventToolShellOutput` 时查找此映射以获取 session_id 和 item_id

**验收**：执行 `ls -la` 时，前端工具卡片在执行期间实时显示输出行

### Phase 3: 交互式终端后端

**范围**：Local PTY Manager + Cloud Terminal Cache + REST API

| 子任务 | 文件 | 说明 |
|--------|------|------|
| P3.1 | `Cargo.toml` (agentdash-local) | 引入 `portable-pty` 依赖 |
| P3.2 | `agentdash-local/src/terminal_manager.rs` | 新建 `TerminalManager` — PTY 创建、stdin 写入、resize、kill、输出读取循环 |
| P3.3 | `agentdash-local/src/command_handler.rs` | 注册 TerminalManager，路由 terminal.* 命令 |
| P3.4 | `agentdash-application/src/session/terminal_cache.rs` | 新建 `SessionTerminalCache` — session 级终端运行时状态 |
| P3.5 | `agentdash-application/src/session/hub.rs` | SessionHub 集成 terminal_cache，提供 terminal 管理接口 |
| P3.6 | `agentdash-api/src/relay/ws_handler.rs` | 处理 `EventTerminalOutput`、`EventTerminalStateChanged` |
| P3.7 | `agentdash-api/src/routes/terminal.rs` | 新建 Terminal REST API 路由 |
| P3.8 | `agentdash-api/src/relay/ws_handler.rs` | backend 断连时调用 `SessionTerminalCache::handle_backend_disconnect()` |

**TerminalManager 设计要点**：
```rust
pub struct TerminalManager {
    terminals: HashMap<String, ManagedTerminal>,
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

struct ManagedTerminal {
    terminal_id: String,
    pty_pair: PtyPair,           // portable-pty master + child
    child: Box<dyn Child>,
    read_task: JoinHandle<()>,   // 持续读取 master 输出的异步任务
}

impl TerminalManager {
    pub fn spawn(&mut self, payload: TerminalSpawnPayload) -> Result<TerminalSpawnResponse>;
    pub fn write_input(&self, terminal_id: &str, data: &[u8]) -> Result<()>;
    pub fn resize(&self, terminal_id: &str, cols: u16, rows: u16) -> Result<()>;
    pub fn kill(&mut self, terminal_id: &str) -> Result<()>;
}
```

**验收**：通过 REST API 创建终端，通过 SSE 接收终端输出，输入 `ls` 后看到输出

### Phase 4: 前端终端面板

**范围**：xterm.js 终端 Tab + Zustand Store + SSE 连接

| 子任务 | 文件 | 说明 |
|--------|------|------|
| P4.1 | `package.json` | 引入 `@xterm/xterm` + `@xterm/addon-fit` + `@xterm/addon-web-links` |
| P4.2 | `frontend/src/stores/terminalStore.ts` | 新建 Terminal Zustand Store |
| P4.3 | `frontend/src/services/terminal.ts` | 新建 Terminal API 客户端 |
| P4.4 | `frontend/src/features/workspace-panel/tab-types/terminal-tab.tsx` | 替换占位符为完整 xterm.js 实现 |
| P4.5 | `frontend/src/features/workspace-panel/tab-types/terminal-tab.tsx` | 从 session SSE 接收 terminal_output 事件，路由到 xterm.js |
| P4.6 | `frontend/src/features/workspace-panel/tab-types/terminal-tab.tsx` | 用户按键 → POST input API |
| P4.7 | `frontend/src/features/workspace-panel/tab-types/terminal-tab.tsx` | resize 观察 → POST resize API |
| P4.8 | `useSessionStream.ts` | 拦截 terminal_output 类型的 platform 事件，转发给 terminalStore 而非创建 displayEntry |

**xterm.js 集成要点**：
- 终端输出通过 session SSE 的 platform BackboneEvent 推送（kind = `terminal_output`）
- `useSessionStream` 拦截 `terminal_output` 事件，不作为 chat entry 渲染，而是通过 terminalStore dispatch 给对应终端实例
- 用户键入通过 `xterm.onData()` 回调 → `POST /api/sessions/{id}/terminals/{tid}/input`
- Container resize 通过 `addon-fit` + ResizeObserver → `POST /api/sessions/{id}/terminals/{tid}/resize`

**验收**：在工作面板打开终端 Tab，可以交互式输入命令，看到带 ANSI 颜色的输出

### Phase 5: 集成与联动

**范围**：Promote 机制、Agent 上下文、断连处理

| 子任务 | 文件 | 说明 |
|--------|------|------|
| P5.1 | `SessionToolCallCard.tsx` | 为 commandExecution 卡片添加 `[⬈ 在终端中查看]` 按钮 |
| P5.2 | `SessionToolCallCard.tsx` + `WorkspacePanel` | Promote 逻辑：打开/激活 terminal Tab 并关联到对应命令 |
| P5.3 | `agentdash-api/src/relay/ws_handler.rs` | backend 断连事件处理：遍历 session terminal cache，标记所有关联终端为 Lost |
| P5.4 | `agentdash-application/src/session/` | 终端生命周期事件作为 platform BackboneEvent 推送：`terminal_spawned`, `terminal_exited`, `terminal_lost`, `terminal_killed` |
| P5.5 | `agentdash-application/src/session/` | Agent 上下文注入：在 turn 开始时，将活跃终端列表作为上下文提供给 Agent |
| P5.6 | 前端 | terminal_lost 事件在终端 Tab 中显示断连提示 |

**Promote 机制设计**：
- 串行 shell_exec 命令在执行期间，工具卡片上显示 `[⬈ 在终端中查看]` 按钮
- 点击后：
  1. 调用 `expandWorkspacePanel("terminal", uri)` 打开/激活终端 Tab
  2. Terminal Tab 显示该命令的 accumulatedText（已有输出），并继续接收 `command_output_delta`
  3. 对于已完成的命令，以只读方式展示输出
- **不创建新 PTY**：Promote 仅是将命令输出在终端面板中可视化，不改变命令的执行方式

**Agent 上下文注入**：
- 当终端状态发生关键变化时（Lost/Killed），通过 `SessionHub::inject_notification()` 推送 platform 事件
- 事件 payload 包含 `terminal_id`、`state`、`exit_code`（如有）
- 前端在 SystemEventCard 中渲染终端生命周期提示

## 6. 依赖关系

```
Phase 1 (协议) ─────────┬──────────────┐
                         │              │
                    Phase 2 (串行流式)  Phase 3 (PTY 后端)
                         │              │
                         │         Phase 4 (前端终端)
                         │              │
                         └──────┬───────┘
                                │
                           Phase 5 (集成)
```

- Phase 2 和 Phase 3 可在 Phase 1 完成后并行开发
- Phase 4 依赖 Phase 3 的 API 和事件定义
- Phase 5 依赖 Phase 2 + Phase 4

## 7. 涉及文件清单

### 新建文件
| 文件路径 | 说明 |
|----------|------|
| `crates/agentdash-local/src/terminal_manager.rs` | PTY 终端管理器 |
| `crates/agentdash-application/src/session/terminal_cache.rs` | Session 终端运行时缓存 |
| `crates/agentdash-api/src/routes/terminal.rs` | Terminal REST API |
| `frontend/src/stores/terminalStore.ts` | Terminal Zustand Store |
| `frontend/src/services/terminal.ts` | Terminal API 客户端 |
| `frontend/src/types/terminal.ts` | Terminal TypeScript 类型 |
| `frontend/src/features/session/ui/CommandExecutionCard.tsx` | 专用命令执行卡片 |

### 主要修改文件
| 文件路径 | 修改内容 |
|----------|----------|
| `crates/agentdash-relay/src/protocol.rs` | 新增 relay 消息类型 |
| `crates/agentdash-local/src/tool_executor.rs` | 新增 `shell_exec_streaming()` |
| `crates/agentdash-local/src/command_handler.rs` | 路由 terminal 命令，使用 streaming shell_exec |
| `crates/agentdash-api/src/relay/ws_handler.rs` | 处理新的 relay 事件消息 |
| `crates/agentdash-api/src/mount_providers/relay_fs.rs` | exec 改造为支持流式 |
| `crates/agentdash-application/src/session/hub.rs` | 集成 terminal_cache |
| `crates/agentdash-application/src/vfs/tools/fs.rs` | ShellExecTool 支持流式回调 |
| `crates/agentdash-spi/src/mount.rs` | ExecRequest/ExecResult 扩展（可选） |
| `frontend/src/features/workspace-panel/tab-types/terminal-tab.tsx` | 完整 xterm.js 实现 |
| `frontend/src/features/session/ui/SessionToolCallCard.tsx` | 流式 UX + Promote 按钮 |
| `frontend/src/features/session/model/useSessionStream.ts` | 拦截 terminal_output 事件 |
| `frontend/src/pages/SessionPage.tsx` | 终端事件分发 |

## 8. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| portable-pty 在 Windows 上的兼容性 | PTY 创建失败 | Windows 使用 ConPTY，需测试；fallback 到 pipe 模式 |
| 高频 terminal_output 事件的 SSE 带宽 | 前端卡顿 | 本地 output 合并（100ms debounce），单次最大 4KB chunk |
| Session 终端泄露（未清理） | 资源浪费 | Session 关闭时自动 kill 所有关联终端，定期 GC |
| call_id 路由映射内存增长 | OOM | 命令完成后立即清理映射，TTL 兜底 |
| 大量终端输出的全量持久化 | 数据库膨胀 | 首期接受，后续 backlog: 摘要持久化 + 输出 TTL 淘汰 |

## 9. 后续 Backlog

- [ ] 终端输出持久化优化（摘要模式、TTL 淘汰）
- [ ] Agent 终端管理工具（`spawn_terminal`、`get_terminal_output`）
- [ ] 终端会话共享（多用户查看同一终端）
- [ ] 终端录制与回放
- [ ] 终端搜索（在输出中搜索文本）
