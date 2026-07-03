# 平台过程诊断（diagnostics）规范

> AgentDashboard 后端**平台过程诊断**的统一约定。诊断回答「平台进程在干什么、哪里出问题」，
> 与领域数据（业务上发生了什么）是两件事。

---

## 边界澄清

诊断入口针对**平台过程诊断**，不等于领域事件。以下是合法的领域概念，**保持原样、不折叠进诊断入口**：

- 会话事件（session events）
- Context Audit（上下文审计）
- Lifecycle / ExecutionAnchor / RuntimeHealth（控制面状态）
- Shell 工具输出流

领域数据回答「业务上发生了什么」；诊断日志回答「平台进程在干什么、哪里出问题」。不要把领域数据塞进 `diag!`，也不要用领域通道承载平台诊断。

---

## 唯一入口：`diag!`

平台过程诊断**只走** `agentdash_diagnostics::diag!` 宏。禁止裸调用 `tracing::{info,warn,error,debug,trace}!`（由 clippy 守门，见下）。

```rust
use agentdash_diagnostics::{diag, Subsystem};

diag!(Info, Subsystem::Relay, backend_id = %bid, "本机后端注册完成，进入消息循环");
```

签名：`diag!(<Level>, <Subsystem>, <field>=<val>, ..., "message")`

- `<Level>`：`Error | Warn | Info | Debug | Trace`。
- `<Subsystem>`：必填，[`Subsystem`] 枚举值，渲染为 event 的 `subsystem` 字段。
- `<field>=<val>`：结构化字段，支持 tracing 字段语法（`field = %x` / `field = ?x` / `field = val`）。
- `"message"`：消息字面量（可带格式化参数）。

实现要点：`diag!` 展开为 `tracing::event!`（**不是** `tracing::info!`），因此与 clippy `disallowed-macros` 守门共存——facade 自身不被误伤。

> span 宏（`tracing::info_span!` 等）不在禁用名单，仍可正常使用；`DiagnosticLayer` 会从当前 span 抽取已知关联字段。

---

## 标准错误诊断：`diag_error!`

需要记录错误对象的诊断入口使用 `agentdash_diagnostics::diag_error!`，并配套 `DiagnosticErrorContext` 构造 operation / stage：

```rust
use agentdash_diagnostics::{diag_error, DiagnosticErrorContext, Subsystem};

let context = DiagnosticErrorContext::new("agent_run.fork", "materialization");

diag_error!(
    Error,
    Subsystem::AgentRun,
    context = &context,
    error = &error,
    run_id = %run_id,
    client_command_id = %client_command_id,
    "AgentRun fork materialization failed"
);
```

`diag_error!` 统一注入以下字段：

- `operation`：稳定操作名，如 `agent_run.fork`。
- `stage`：操作内失败阶段，如 `materialization`、`receipt_claim`。
- `detail`：由 operation / stage / error 组成的排障摘要。
- `error` / `error_debug`：错误的 Display 与 Debug 表达。

调用点继续补充 `run_id`、`session_id`、`backend_id` 和该 use case 自有的结构化字段，作为上下文事实的唯一结构化来源。这样做的原因是错误诊断的消息结构属于 diagnostics facade 的公共契约，业务模块只提供上下文事实，不各自拼装错误消息模板，也不重复嵌套同一批上下文字段。

---

## 级别约定

| 级别 | 使用场景 |
|------|----------|
| `Error` | 需要人工干预的错误（关键业务逻辑异常、不可恢复失败） |
| `Warn` | 可恢复的异常 / 降级 / 失败的非关键路径（脚本执行失败、投影加载失败、gate 超时） |
| `Info` | 重要生命周期 / 关键决策点（dispatch 进入与完成、编译完成、规则命中、skill 加载完成） |
| `Debug` | 开发调试 / 高频触发点（每次 hook evaluate 进入、命令前置检查细节） |
| `Trace` | 极细粒度追踪 |

不要在热循环 / 高频路径上打 `Info` 及以上级别；高频点位用 `Debug` 并依赖 `EnvFilter` 控级。

---

## Subsystem 取值与归类口径

`Subsystem` 取值为稳定小写字符串，供查询端点按列过滤。取值口径：按**调用点所属子系统**赋值，不强求与 crate / 模块路径一致。

| 变体 | 渲染值 | 语义 / 归类 |
|------|--------|------------|
| `Relay` | `relay` | Relay 消息路由 / 后端注册（`agentdash-relay`、`relay/ws_handler`） |
| `SessionLaunch` | `session_launch` | 会话启动链路（`session/launch/*`） |
| `AgentRun` | `agent_run` | AgentRun 执行（`agentdash-application-agentrun`） |
| `Lifecycle` | `lifecycle` | 生命周期调度与状态转换（dispatch、gate） |
| `Workflow` | `workflow` | 工作流编排（graph 编译 / orchestration 规划） |
| `Hooks` | `hooks` | Hook 触发、命中、失败 |
| `Skill` | `skill` | 技能发现与装配 |
| `Reconcile` | `reconcile` | 对账 / 状态收敛（`reconcile/*`） |
| `Cron` | `cron` | 定时任务 / Cron |
| `Auth` | `auth` | 鉴权 / 认证 |
| `Vfs` | `vfs` | 虚拟文件系统 |
| `Infra` | `infra` | 基础设施 / 通用（DB、配置、启动等无更具体归类时） |
| `Mcp` | `mcp` | MCP 协议相关 |
| `Api` | `api` | HTTP API 层（路由、中间件） |

新增子系统时在 `agentdash-diagnostics::Subsystem` 枚举里加变体并补 `as_str` 映射，再在此表登记。

---

## 关联字段约定

热点诊断路径应尽量带上标准关联字段，方便端点按列过滤与回溯一次执行的时间线：

- `session_id` — 运行时会话 id。
- `run_id` — LifecycleRun id。
- `backend_id` — 后端 id。

用 `field = %x` 语法（如 `session_id = %sid`）。`DiagnosticLayer` 的 visitor 会同时从 event 字段与当前 span 字段中抽取这三个已知 key，填进 `DiagnosticRecord` 的专用列。其余结构化字段（`gate_id`、`node_path`、`orchestration_id` 等）落进通用 `fields` map。

---

## 日志落地（仅 `agentdash-api` 进程）

订阅器只在 `agentdash-api/src/main.rs` 装配，library 侧不调用 `.init()`。三层：

1. **stdout（pretty）** — 保留现状，`EnvFilter` / `RUST_LOG` 控级别，默认 `info`。
2. **JSON line 滚动文件** — `tracing-appender` 按天滚动，目录取环境变量 `AGENTDASH_LOG_DIR`，默认 `./logs/`。进程重启后历史文件可查。发布前不自动清理，靠外部 logrotate。
3. **DiagnosticLayer（环形缓冲）** — 有界 `VecDeque`（默认容量 `DEFAULT_CAPACITY`），`Arc` 共享进 `AppState`，供查询端点读。

> `agentdash-local` / `agentdash-local-tauri` 也用 `diag!`（全 workspace 统一），但它们的 main 保持原 `fmt().init()`，**不接** 文件层 / 缓冲层——不产文件、不暴露查询。诊断暴露只在 api 进程。

---

## 查询端点

`GET /api/diagnostics` 是 public API 下的只读排障入口。query 参数：`subsystem`、`session_id`、`run_id`、`backend_id`、`level`（最低级别）、`since_ms`、`limit`（有上限设防）。

**语义**：端点服务「近期」诊断（读环形缓冲，进程重启清空）；**历史完整记录在 JSON 文件**（端点不解析文件）。不要把它误解为全量历史 API。

---

## clippy 守门（防回退）

workspace 根 `clippy.toml` 用 `disallowed-macros` 禁止裸 tracing 事件宏：

```toml
disallowed-macros = [
  { path = "tracing::info",  reason = "用 agentdash_diagnostics::diag! 统一平台过程诊断入口" },
  { path = "tracing::warn",  reason = "用 diag!" },
  { path = "tracing::error", reason = "用 diag!" },
  { path = "tracing::debug", reason = "用 diag!" },
  { path = "tracing::trace", reason = "用 diag!" },
]
```

`diag!` 展开为 `tracing::event!`（不在名单），span 宏 `info_span!` 等也不在名单，故 facade 与迁移后代码均合规。绕过 facade 的裸宏会在 `cargo clippy -- -D warnings` 失败（用项目 `pnpm run backend:clippy`）。
