# 统一后端可观测层

## Goal

临近 release，**平台自身过程**的诊断日志没有统一入口，散落在各处的 `tracing` 调用 + 临时状态检测里，重启即丢、无法被正确查询/展示，很多运行状态只能靠在犄角旮旯翻找间接推断。目标是给平台过程诊断收一个**统一的日志入口**，确保诊断信息被结构化记录、持久化、并能被正确查询/展示。

**边界澄清（重要）**：本任务针对的是"平台过程诊断"，不等于领域事件。以下是合法的领域概念，保持原样、不折叠进诊断入口：
- 会话事件（session events）
- Context Audit（上下文审计）
- Lifecycle / ExecutionAnchor / RuntimeHealth（控制面状态）
- Shell 工具输出流

诊断入口与这些领域数据是两件事：领域数据回答"业务上发生了什么"，诊断日志回答"平台进程在干什么、哪里出问题"。

## Confirmed Facts（来自代码勘察）

后端技术栈：Rust + Axum + Tokio + PostgreSQL（sqlx），日志框架为 `tracing` + `tracing-subscriber`。

当前可观测/状态信息分裂为多条通道：

| 通道 | 存储 | 持久化 | 暴露方式 |
|---|---|---|---|
| `tracing` 日志 | stdout（`agentdash-api/src/main.rs:6` 初始化，`RUST_LOG` 控级别，默认 info） | ❌ 重启即丢 | 控制台/容器日志；HTTP 有 `TraceLayer` |
| Context Audit Bus | 进程内环形缓冲，每 session 2000 条（`agentdash-application/src/context/audit.rs`） | ❌ session 结束丢 | `/sessions/{id}/context/audit`，前端 Context Inspector 3s 轮询 |
| Backend Runtime Events | broadcast（容量 256，内容仅 backend_id） | ❌ | 前端订阅（`relay/ws_handler.rs:580`） |
| Shell 输出 | mpsc 直接路由（`shell_output_registry.rs`） | ❌ 纯流式 | 前端实时显示 |
| Lifecycle / ExecutionAnchor / RuntimeHealth | PostgreSQL | ✅ 有历史 | DB / API（控制面状态，非"发生了什么"日志） |
| Terminal 状态 | 进程内 HashMap（`terminal_cache.rs`） | ❌ | API（仅活跃） |

核心痛点：
1. 运行时诊断信息（tracing / context audit / shell 输出）无持久化，重启即丢。
2. 状态源分裂，拼一次执行的完整时间线需同时查 stdout + 内存 audit + DB lifecycle + broadcast + cache，没有单一回溯入口。
3. tracing 用法完全无收口：无共享 facade、无 `#[instrument]`、字段（session_id 等）各模块各拍，只 pretty-print 到 stdout、无 JSON、无文件、无查询。

诊断埋点分布（实测，非 subagent 归纳）：全 workspace 431 处 / 106 文件。**核心 usecase 埋点其实很密**——`agentdash-application` 48、`agentdash-application-runtime-session` 47、`agentdash-application-agentrun` 51、`relay/ws_handler` 38、`pi_agent/connector` 27。但分布**极不均**：`agentdash-application-lifecycle`(4)、`agentdash-application-workflow`(1)、`agentdash-application-hooks`(0)、`agentdash-application-skill`(0) 几乎是哑的——这是"靠翻找间接检测"的主要来源（特定子系统不出声，而非整体稀疏）。

## Requirements

- 建立**统一诊断 facade**（方案 B）：一个统一的 `diag!` 宏 / 薄封装作为平台过程诊断的唯一入口，强制结构化字段——至少 `subsystem`（子系统）+ `level` + message，并约定标准关联字段（`session_id` / `run_id` / `backend_id` 等，按场景可选）。
- **全量迁移** `agentdash-api` 进程涉及的现有 `tracing::{info,warn,error,debug}!` 调用点到统一 facade：
  - 换宏部分是机械替换（可脚本化）。
  - 在热点诊断路径上人工补齐关联字段（session/run/backend id 等），这是 B 的增量价值。
- 诊断日志**持久化**为结构化（JSON line）滚动日志文件（按大小/天滚动）。
- 提供一个**只读查询端点**（最小实现，供后续 admin gateway 消费；当前不做前端面板）。
- **CI 硬堵防回退**：新建 workspace 级 `clippy.toml`，用 `disallowed-macros` 禁止裸调用 `tracing::{info,warn,error,debug}!`（仅 facade 自身放行），让绕过 facade 的写法在 CI 失败。
- **定向补埋点**：不对业务层普遍补，只给实测几乎哑掉的子系统补关键路径诊断——`agentdash-application-lifecycle`、`agentdash-application-workflow`、`agentdash-application-hooks`、`agentdash-application-skill`。

## Acceptance Criteria

- [ ] 存在统一诊断 facade（`diag!`），强制 `subsystem` + level + message，关联字段（session_id/run_id/backend_id）有约定。
- [ ] 全 workspace 现有 `tracing::{info,warn,error,debug}!` 调用点已迁移到 facade；`cargo clippy` 在出现裸 tracing 宏时失败。
- [ ] `agentdash-api` 进程诊断日志双写：stdout（pretty）+ JSON line 滚动文件（`AGENTDASH_LOG_DIR` 默认 `./logs/`，按天滚动）；进程重启后历史文件可查。
- [ ] 存在只读查询端点（secured_api 下），可按 subsystem / 关联 id / level / 时间范围过滤近期诊断。
- [ ] lifecycle / workflow / hooks / skill 四个哑子系统的关键路径补上了诊断埋点。
- [ ] 现有领域通道（context audit、session events、lifecycle 控制面、shell 输出）行为不变、不被牵动。
- [ ] `cargo build` / `cargo clippy` / 现有测试通过。

## Out of Scope

- **只覆盖 `agentdash-api` 中心进程**；`agentdash-local`（本地后端/工具执行进程）与 `agentdash-local-tauri`（桌面）本次不动，跨进程诊断汇聚留作后续迭代。
- 当前 `app-web` 前端不新增诊断面板（未来 admin gateway 再补）。
- 不改动 / 不折叠领域数据通道：**context audit（会话级上下文检视）、session events、lifecycle / ExecutionAnchor / RuntimeHealth、shell 工具输出流**。
- 不引入外部日志聚合（Loki/ELK 等）。

## Decisions（brainstorm 已收敛）

- 范围：只覆盖 `agentdash-api` 进程的诊断**暴露**（文件落地 + 查询端点）；但 facade 与迁移铺到全 workspace（方案 7-A）。
- 形态：方案 B——统一 `diag!` facade + 全量迁移调用点（机械换宏可脚本化，热点路径人工补关联字段）。
- 防回退：clippy `disallowed-macros` 硬堵裸 tracing 宏。
- 落地：`AGENTDASH_LOG_DIR` 默认 `./logs/`，按天滚动，发布前不自动清理（文档注明靠外部 logrotate），stdout(pretty) + 文件(JSON line) 双写。
- 补埋点：定向补 lifecycle / workflow / hooks / skill 四个哑子系统。

## Open Questions

- （无阻塞项；细节见 design.md / implement.md）
