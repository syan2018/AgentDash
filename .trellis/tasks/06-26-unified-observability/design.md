# 设计：统一后端可观测层

## 架构概览

```
                       ┌─────────────────────────────────────────────┐
   各 crate 调用点  →  │  diag! 宏 (agentdash-diagnostics)            │
   (含 application,    │   - 强制 subsystem 字段                       │
    runtime-session,   │   - 约定 session_id/run_id/backend_id 等      │
    relay, ...)        │   - 展开为 tracing::event!（不是 tracing::info!）│
                       └───────────────┬─────────────────────────────┘
                                       │ 标准 tracing event（带结构化字段）
                                       ▼
                       tracing-subscriber Registry（仅 agentdash-api main 装配）
                        ├─ Layer 1: fmt（stdout, pretty, EnvFilter 控级别）  ← 保留现状
                        ├─ Layer 2: JSON-line file（tracing-appender, daily roll, AGENTDASH_LOG_DIR）
                        └─ Layer 3: DiagnosticLayer（有界环形缓冲，Arc 共享进 AppState）
                                       │
                                       ▼
                       GET /api/diagnostics（secured_api）→ 读环形缓冲，过滤后返回
```

`agentdash-local` / `agentdash-local-tauri` 也用 `diag!` 宏（同一共享 crate），但它们的 main 保持原 `fmt().init()` 订阅器——**不接 Layer 2/3**，因此不产文件、不暴露查询。这守住"诊断暴露只在 api 进程"的范围，同时宏统一与 clippy 防回退全局生效。

## 边界与契约

### 新 crate：`agentdash-diagnostics`
低层、零业务依赖（仅 `tracing` / `tracing-subscriber` / `serde` / `serde_json`）。导出：

- `diag!` 宏。签名约定：`diag!(<level>, <subsystem>, <field>=<val>, ..., "message")`
  - `<level>`：`Error|Warn|Info|Debug|Trace`（或直接复用 `tracing::Level`）。
  - `<subsystem>`：必填。用 `Subsystem` 常量/枚举（如 `Relay`、`SessionLaunch`、`Lifecycle`、`Workflow`、`Hooks`、`Skill`、`AgentRun`、`Reconcile`、`Cron`、`Auth`、`Infra` …），渲染为 `subsystem` 字段。
  - 展开为 `tracing::event!(level, subsystem = …, 其余字段, message)`。**关键：展开成 `event!` 而非 `info!`，从而不触发 clippy 对 `tracing::info!` 等的封禁。**
- `DiagnosticRecord`：序列化结构（at_ms、level、subsystem、message、fields: Map、target、可选 session_id/run_id/backend_id）。
- `DiagnosticLayer`：实现 `tracing_subscriber::Layer`，把 event 落入 `Arc<RwLock<VecDeque<DiagnosticRecord>>>`（有界，默认容量可配，如 4096）。提供 `query(filter) -> Vec<DiagnosticRecord>`。
- `DiagnosticBuffer`（= 上述 Arc 句柄）：放进 `AppState` 给查询端点读。

> 关联字段（session_id/run_id/backend_id）落地方式：优先靠 `diag!` 显式字段；`DiagnosticLayer` 的 visitor 同时从 event 字段与当前 span 字段中提取这些已知 key，填进 `DiagnosticRecord` 的专用列，方便端点按列过滤。

### 订阅器装配（`agentdash-api/src/main.rs`）
由当前：
```rust
tracing_subscriber::fmt().with_env_filter(...).init();
```
改为 registry 组合：
```rust
use tracing_subscriber::{prelude::*, Registry, EnvFilter, fmt};
let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
let (file_layer, _guard) = build_json_file_layer();  // tracing-appender non_blocking，guard 需持有到进程结束
let diag_buffer = DiagnosticBuffer::new(DEFAULT_CAP);
Registry::default()
    .with(env)
    .with(fmt::layer())                    // stdout pretty，保留
    .with(file_layer)                      // JSON line 文件
    .with(diag_buffer.layer())             // 环形缓冲
    .init();
```
- `_guard`（`tracing_appender::non_blocking::WorkerGuard`）必须在 `main` 生命周期内持有，否则后台写线程提前退出丢日志。
- `diag_buffer` clone 进 `run_server` → `AppState`。
- 文件层：`tracing_appender::rolling::daily(log_dir, "agentdash-api.log")` + `fmt::layer().json().with_writer(non_blocking)`。`log_dir = env("AGENTDASH_LOG_DIR").unwrap_or("./logs")`。

> 注意 `run_server` 当前签名与 main 的关系：`AppState` 构造在 `bootstrap`，需把 `DiagnosticBuffer` 从 main 透传进去（新增构造参数或 builder 字段）。订阅器 init 仍只在 main，library 不 init（避免测试/嵌入重复 init）。

### 查询端点
- 新模块 `agentdash-api/src/routes/diagnostics.rs`，`router()` merge 进 `secured_api`（`routes.rs:70`），自动套 `authenticate_request` 鉴权。
- `GET /api/diagnostics`，query 参数：`subsystem`、`session_id`、`run_id`、`backend_id`、`level`(最低级别)、`since_ms`、`limit`(默认/上限设防)。
- 实现：从 `AppState.diagnostics` 读 `VecDeque` 快照，按参数过滤，按时间倒序截断 `limit` 返回 JSON。
- **语义**：端点服务"近期"诊断（环形缓冲，重启清空）；**历史完整记录在 JSON 文件**（端点不解析文件，发布前足够）。在响应或文档里注明这一点，避免误解为全量历史 API。

## clippy 防回退
workspace 根新增 `clippy.toml`：
```toml
disallowed-macros = [
  { path = "tracing::info",  reason = "用 diag! 统一诊断入口" },
  { path = "tracing::warn",  reason = "用 diag!" },
  { path = "tracing::error", reason = "用 diag!" },
  { path = "tracing::debug", reason = "用 diag!" },
  { path = "tracing::trace", reason = "用 diag!" },
]
```
- `diag!` 展开为 `tracing::event!`，不在禁用名单，故 facade 自身与迁移后代码均合规。
- `disallowed-macros` 为 clippy `nursery`/`restriction`？实为稳定 lint（clippy.toml 配置项），`cargo clippy -- -D warnings` 即可在 CI 失败。
- 需在 CI 脚本确保 `cargo clippy` 对全 workspace 执行（确认现有 CI 是否已有，没有则补一步）。

## 数据流（典型：后端注册诊断）
迁移前（`relay/ws_handler.rs:179`）：
```rust
tracing::info!(backend_id = %bid, "本机后端注册完成，进入消息循环");
```
迁移后：
```rust
diag!(Info, Relay, backend_id = %bid, "本机后端注册完成，进入消息循环");
```
→ `tracing::event!` 携带 `subsystem="relay"` + `backend_id` → 三层订阅器各自处理：stdout 打印 / 写 JSON 文件行 / 进环形缓冲（backend_id 抽进专列）。→ `GET /api/diagnostics?subsystem=relay&backend_id=…` 可查。

## 兼容性与风险
- **行为零变化**：宏只改产出管线，不改控制流；`EnvFilter`/`RUST_LOG` 语义保留。这些日志此前无人消费（用户确认），迁移无回归面。
- **重复 init 风险**：library 侧不得调用 `.init()`；仅 `main` 装配。检查现有测试是否曾依赖 fmt init（一般用 `try_init`）。
- **性能**：环形缓冲 + non_blocking 文件写均为低开销；缓冲有界防内存膨胀。
- **subsystem 归类口径**：迁移时按"调用点所属子系统"赋值，不强求与模块路径一致；提供一份 subsystem 取值约定文档（spec）。
- **`agentdash-local` 迁移**：同样换宏（全 workspace），但订阅器不变——它继续 stdout，不产文件。属预期，不算范围外。

## 回滚形状
- 改动可分层回滚：查询端点（删 route）→ 文件层（main 去掉 file_layer）→ DiagnosticLayer。最坏情况保留 `diag!`+迁移（纯产出等价于原 tracing），仅去掉新订阅层，行为回到现状。
- clippy 规则可单独 revert（删 clippy.toml）而不影响运行时。
