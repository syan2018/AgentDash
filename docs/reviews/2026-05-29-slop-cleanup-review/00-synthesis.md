# 2026-05-29 架构 slop 清理 review — 综合

> 七路并行 subagent review（session / vfs-workflow / application 其余 / domain-spi / infrastructure / api-edges / frontend）的汇总。session、vfs-workflow、application 其余三块各做了双盲复核，结论互相印证。
>
> 落地入口：Trellis 任务树 `05-29-architecture-slop-cleanup`（parent）及其 7 个 child。

## 总评

架构骨架是对的（DDD 分层 + 策略可插拔 + 多后端/relay 方向成立），问题在执行层失控：同一套病灶在几乎每个模块重复发作。`application`(90k) 与 `app-web`(75k) 是两个火山口，吃掉项目 75% 体量。

## 模块质量速览

| 模块 | LOC | 评级 | 一句话 |
|---|---|---|---|
| domain | 14k | 中 | 骨架清晰，但泄漏 Extension manifest、双 lifecycle 死字段、错误模型过粗 |
| application | 90k | 差 | 层职责失守：装基础设施、装领域规则、装存储实现，god file 成群 |
| infrastructure | 18k | 差 | sqlite/postgres 82% 逐行重复；repo 混业务规则 |
| spi | 6k | 中 | `SessionPersistence` 60+ 方法手抄；枚举 boilerplate |
| contracts | 3k | 中 | `session.rs` 800 行投影逻辑放错层 |
| api | 26k | 偏差 | route handler 塞业务逻辑 + N+1；71 处样板 `map_err` |
| executor/agent | 16k | 偏差 | 4 bridge spawn 脚手架逐字复制；审批链路 mock 硬写 |
| relay/mcp/local | 15k | 偏差 | MCP HTTP 连接逻辑三处各写一遍 |
| app-web | 75k | 差 | 19 store 全手写 server-state；store 绕过 service 直连 api；god component |

## 六类系统性病灶

### 病灶 1 — application 层是基础设施的垃圾桶（4 模块独立点名）
- `skill_asset/service.rs:328` 直接 `reqwest::Client` 抓 GitHub/ClawHub/skills.sh（1845 行里 ~1400 是 HTTP 客户端）
- `mcp_preset/probe.rs:13` 直接 new `rmcp` StreamableHttp 实时探测
- `hooks/script_engine.rs:5` 直接嵌 `rhai` 解释器
- `workflow/agent_executor.rs:647` 直接 `reqwest` + `tokio::process::Command`
- `session/memory_persistence.rs`（1466 行内存存储）整个塞在 application，且无 `#[cfg(test)]`，编进 release
- 药方：定义 `RemoteSkillSource`/`McpProbeTransport`/`HookScriptEvaluator`/`FunctionRunner` 四 port，实现下沉 infrastructure

### 病灶 2 — 双轨 lifecycle 并存（domain/vfs-workflow/infra 三处独立发现）【决策：Activity 为目标，删 Step】
- domain `entity.rs:299` `step_states` 注释"不再上线"却仍被 `activate_step/complete_step/fail_step` ~300 行读写；新 `activity_state` 并存
- workflow `catalog.rs:143` vs `:178`：`WorkflowCatalogService` 与 `ActivityLifecycleCatalogService` 各实现一遍 `upsert`
- infra `workflow_repository.rs`：两实体挤同一张表，靠 `entry_activity_key <> ''` magic string 区分（L197/213/229/272/373）

### 病灶 3 — 装配流水线复制成平行宇宙（session 双盲点名）
- `assembler.rs`(2654, god module) `compose_owner_bootstrap` 与 `construction_planner.rs` `plan_*_context_query` 把六步装配链各手写一遍（`assembler.rs:849` / `construction_planner.rs:155,303`）
- `SessionConstructionPlan` 把 `vfs` 镜像到 3 字段（`construction.rs:35/63/114`），靠 `validate_for_launch` 运行期断言防漂移
- 药方：抽 `SessionSurfaceResolver`（`OwnerScope → ResolvedSessionSurface`），删 600–800 行

### 病灶 4 — 同一概念散落 / 命名漂移
- 能力状态机散 4 处：`capability/resolver` + `session/capability_state` + `session/capability_projection` + `session/dimension/`，并行两套 dimension trait（`CapabilityDimensionModule` vs `DimensionDelta`）
- `runtime*` 命名簇：`runtime.rs`/`runtime_bridge.rs`/`runtime_gateway/`/`backend_transport.rs` 五个 runtime 语义
- `RelayVfsService`(relay_service.rs:59) 根本不碰 relay，纯命名误导
- extension 关注点切 4 个顶层文件（package/runtime/management + runtime_gateway/extension_actions）
- 重复类型：`McpTransportConfig`（domain + spi 各一份，36 文件引两版）；`AgentTemplateConfig` ≈ `AgentPresetConfig`；`RawSkillUploadFile` ≈ `SkillAssetFileInput`

### 病灶 5 — 海量逐行复制
- infra：sqlite/postgres session_repository **82% 重复**（6054 行可砍到 ~1100）
- executor：4 bridge `stream_complete` spawn 脚手架 + HTTP 错误处理逐字复制（anthropic/openai_completions/openai_responses/openai_codex bridge）
- MCP HTTP 连接逻辑三处（`executor/mcp/direct.rs:193` / `local/mcp_client_manager.rs:152` / `local/handlers/mcp_relay.rs:47`）
- spi：`SessionPersistence`(session_persistence.rs:825) 手抄 7 子 trait ~35 方法，应是空 body supertrait
- 前端：`sidebarSessionsStore` 与 `activeSessionsStore` 两份相同代码

### 病灶 6 — 手写 JSON poking + stringly error + 样板 map_err
- `eventing.rs:272` 8 处 `value.get()` 手挖 `context_compacted` envelope；lifecycle mount `writable_port_keys` 靠无类型 metadata 约定跨模块耦合（`provider_lifecycle.rs:347` 校验 vs `workflow/lifecycle/mount.rs:21` 生产）
- `context/`、`routine/`、`hooks/`、companion 大量 `Result<_, String>`
- api routes 71 处 `.map_err(|e| ApiError::Internal(e.to_string()))`，`From` impl 未补全

## 前端专项
1. 无 react-query，19 store 全手写 `isLoading/error/竞态/stale`（最高杠杆）
2. store 直连 `api.client` 绕过 service 层（project/story/workspace mapper 散在 store）
3. `@agentdash/ui` 仅 16% 文件使用，各 feature 自造 primitive
4. god component：`SettingsPageContent.tsx`(2014)、`activity-inspector.tsx`(1304)、`workspace-layout.tsx`(1230)

## 确认「不是问题」（增信，证明结论非无差别开火）
- workflow catalog builtins 已正确 `include_str!` 外置
- `relay_service` 与 `agentdash-relay` 是 facade vs transport，不重复
- `agentdash-executor` 与 `workflow/agent_executor.rs` 不同抽象层，不重叠

## 重构路线图 → 任务映射

| 优先级 | 病灶 | child task |
|---|---|---|
| P0 | 病灶 2 | `05-29-drop-step-lifecycle` |
| P0 | 病灶 4/5/6 低风险部分 | `05-29-dedup-naming-boilerplate` |
| P1 | 病灶 1 | `05-29-app-infra-leak-to-spi` |
| P1 | 病灶 5 (infra) | `05-29-infra-persistence-dedup` |
| P1 | 病灶 3 | `05-29-session-assembly-converge` |
| P2 | 病灶 4 (capability) | `05-29-capability-state-unify` |
| P2 | 前端 | `05-29-frontend-server-state-refactor` |

## 约束
- 预研期未上线：无向后兼容/字段兼容/回退包袱；可激进删除
- 但数据库 schema 变更必须走 migration（如 infra 加 `kind` 列）
