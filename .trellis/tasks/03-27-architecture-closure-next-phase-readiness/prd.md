# 架构收口与稳定化准入 — 统一收口任务

## 背景

2026-03-27 的全仓 review 结论表明，项目已明显脱离"大量重复、全面混乱"的阶段，前后端主干分层方向总体正确，多个 God module 也已经开始被拆解。

但仍有几类问题会直接影响是否适合进入下一阶段（稳定化 / 发布准备）：

- 默认质量门禁还不能稳定代表"项目已通过健康检查"
- Session 执行状态的查询仍有两条不一致的读取路径
- Address Space / runtime tool 在 cloud 与 local 路径上的底层语义仍有分叉，遗留协议链未退场
- `system_context` 架构已设计，但所有真实执行链路传入的都是 `None`
- 前端大组件未拆分，是回归风险源
- 前后端 DTO 命名和部分 UI 入口仍保留兼容层或硬编码
- 领域层高频 `serde_json::Value` payload 缺少显式 schema 和错误边界标准

本任务是**统一收口任务**，合并了以下零碎专项：

| 被吸收任务 | 原状态 | 吸收原因 |
|-----------|--------|---------|
| `03-25-quality-gate-e2e-reality-check` | in_progress (3/4 done) | 剩余 real-relay-e2e 直接在此完成 |
| `03-25-frontend-project-isolation-resume-hardening` | in_progress (3/4 done) | 剩余 page-split 直接在此完成 |
| `03-25-domain-payload-typing-error-model` | planning | 仅在此定义标准，不要求全部实现 |
| `03-23-agent-tooling-redundancy-closure` | planning | workspace_files 处置 + MCP tool identity 收口 |

## Goal

形成一条可执行的"稳定化准入收口任务"：

1. 明确项目进入稳定化阶段前**必须补齐的闭环**
2. 把分散在多个零碎任务中的收口项**统一管理**
3. 每个子任务有明确的完成定义，而非泛泛 review
4. 能清晰回答"项目是否已准备进入稳定化阶段"，并给出可追溯依据

---

## 收口项（按优先级梯队）

### Must-complete：稳定化准入阻塞项

#### M1. 质量门禁可信化

**来源**：原 subtask `trusted-quality-gate` + 已吸收 `03-25-quality-gate-e2e-reality-check`

当前根 `check` 已覆盖 `cargo check` + `cargo test` + `tsc --noEmit` + `frontend:test` + `e2e:test:critical`，但仍不足以代表"项目已通过默认质量要求"。

已完成：
- [x] 默认检查纳入 Rust check/test、frontend typecheck/test、关键 E2E
- [x] E2E 运行环境隔离（每次独立 DB/backend）
- [x] 移除 E2E 对固定盘符和绝对路径的依赖

待完成：
- [ ] 根 `check` 加入 `cargo clippy`（至少 `--workspace -- -D warnings` 级别）
- [ ] 根 `check` 加入 `frontend:lint`（eslint）
- [ ] 真实 relay/local 全链路 E2E 可跑通（不超时、不假绿、验证成功态）
- [ ] multi-agent worktree verify 有统一命令定义（`check` 脚本即为默认 verify）

**完成定义**：执行 `pnpm run check` 后，结果能真实反映项目编译、类型、lint、核心 E2E 的健康状态。

---

#### M2. Session 执行状态单一化

**来源**：原 subtask `session-state-source-of-truth`

代码中已有 `SessionMeta.last_execution_status` 作为持久化终态 SoT，且 `inspect_execution_states_bulk()` 已从 meta 直接读。但单条查询 `inspect_session_execution_state()` 仍在 `hub.rs:658` 调用 `self.store.read_all()` 扫描完整 JSONL 历史来回推终态。两条路径不一致。

待完成：
- [ ] `inspect_session_execution_state()` 改为优先从内存 map 判断 running，否则从 meta 读取终态（对齐 bulk 版本，不再扫 JSONL）
- [ ] cancel / reconcile 流程确认统一走 meta 写入 → 查询 meta 读取
- [ ] 明确文档化 `meta`（执行终态 SoT）与 `history`（可审计的事件流，不参与状态判定）的职责边界

**完成定义**：所有执行状态查询路径统一从 meta 读取，JSONL 历史不再作为状态推断的数据源。

---

#### M3. system_context 注入链落地

**来源**：原 subtask `owner-system-context`

`system_context` 字段已定义在 `SessionStartRequest`、`SessionContext`（connector contract）中，设计意图是将 project/story owner 上下文注入到 system prompt。但 **所有真实执行链路都传 `None`**：

- `task_execution_gateway.rs` → `system_context: None`
- `companion.rs` companion dispatch → `system_context: None`
- 所有测试 → `system_context: None`

待完成：
- [ ] `task_execution_gateway` 从 bootstrap plan 获取并填充 `system_context`
- [ ] story session / project session 启动时填充 `system_context`
- [ ] companion dispatch 继承父 session 的 `system_context`（或从 execution slice 派生）
- [ ] 至少一个集成测试验证 `system_context` 在 connector 侧被接收

**完成定义**：task/story/project 三类 session 启动时，`system_context` 基于 owner 上下文被真实填充，而非全部为 `None`。

---

#### M4. 前端大组件拆分

**来源**：已吸收 `03-25-frontend-project-isolation-resume-hardening` 剩余项

已完成（在原专项任务中）：
- [x] story cache 按 projectId 隔离
- [x] ACP reconnect 时 transcript 与 token usage 保留
- [x] session history store 错误可见性补齐

待完成：
- [ ] `SessionPage` 按 feature 拆分为独立子组件（transcript 区、input 区、meta 区等）
- [ ] `StoryPage` 按 feature 拆分为独立子组件

**完成定义**：两个页面不再是单一 God Component，每个子 feature 可独立修改而不影响其他部分。

---

#### M5. DTO 命名与 UI 入口收敛

**来源**：原 subtask `dto-and-ui-closure`

待完成：
- [ ] 确认全栈业务 DTO 单一命名风格（后端 snake_case，前端类型映射 snake_case）
- [ ] 测试中不再主动容忍 camel/snake 双读兼容，契约漂移直接失败
- [ ] Project Agent / Address Space 等下一阶段会继续扩展的 UI 入口，从硬编码改为由后端能力或元数据驱动

**完成定义**：业务 API 返回 snake_case，前端消费 snake_case 类型，测试不接受双风格漂移，UI 入口不依赖硬编码的能力判断。

---

### Should-complete：重要但不绝对阻塞

#### S1. Address Space 遗留协议与路径语义收口

**来源**：原 subtask `address-space-runtime-unification` + 已吸收 `03-23-agent-tooling-redundancy-closure`

`workspace_files` 遗留协议链仍存在于 8 个 `.rs` 文件中。统一 Address Space surface 已广泛实现，但 cloud/local 在路径归一化、绝对路径处理上仍可能有不同语义。

待完成：
- [ ] `workspace_files.*` 遗留协议链做出明确处置决定（冻结为内部兼容 / 迁移后删除），并在代码中标注
- [ ] cloud/local 的相对路径统一规则、cwd 解析一致性文档化
- [ ] MCP tool 的 runtime 名称、policy 识别名和展示名关系记录到 spec
- [ ] 流程工具（report_workflow_artifact 等）的 authority 与条件注入原则保持一致

**完成定义**：遗留协议有明确处置，路径语义有文档约束，后续实现者不需要猜测 cloud/local 行为差异。

---

#### S2. 领域负载类型化标准定义

**来源**：已吸收 `03-25-domain-payload-typing-error-model`（仅定义标准，不要求全部实现）

待完成：
- [ ] 盘点领域层裸 `serde_json::Value` 的使用面与优先级排序
- [ ] 为 workflow 校验错误定义结构化错误边界标准（枚举 + 上下文字段，替代裸字符串）
- [ ] 至少完成一组高频 payload 的类型化改造样板，供后续参照

**完成定义**：有清晰的盘点清单、错误边界标准定义、一个可参照的改造样板。不要求在本任务内消灭所有 `JsonValue`。

---

### Deferred：延期到下一阶段

以下内容经评估**不阻塞稳定化准入**，延期到下一阶段推进：

- **Companion 协作模型升级**（`03-23-companion-collaboration-upgrade`）：同步等待模式、await 机制、记忆继承
- **Address Space 条目检索补全**（`03-10-extend-address-space-entries`）：picker 闭环、entries API 扩展
- **Source Resolver 扩展**（`03-10-extend-source-resolvers`）：HttpFetch / McpResource / EntityRef 解析器

---

## 与现有任务的关系

本任务**吸收并替代**以下专项任务（被吸收任务应标记为 `merged` 并指向本任务）：

- `03-25-quality-gate-e2e-reality-check` → M1
- `03-25-frontend-project-isolation-resume-hardening` → M4
- `03-25-domain-payload-typing-error-model` → S2
- `03-23-agent-tooling-redundancy-closure` → S1

以下任务**建议归档**（功能扩展，不属于收口阶段）：

- `03-10-extend-address-space-entries`
- `03-10-extend-source-resolvers`

以下任务**保持独立、延期**：

- `03-23-companion-collaboration-upgrade`

---

## 非目标

- 不要求在本任务内一次性重写所有 session / tool / API 模块
- 不要求消灭所有历史遗留 TODO 或所有重复代码
- 不要求全部 `serde_json::Value` 都被类型化
- 不替代 Companion 升级等下一阶段功能设计
- 不追求对所有 E2E 场景的完整覆盖，只覆盖关键链路

## Acceptance Criteria

- [ ] `pnpm run check` 覆盖 clippy + lint + test + 关键 E2E，结果可信
- [ ] Session 执行状态查询全部走 meta，不依赖 JSONL 扫描
- [ ] `system_context` 在 task/story/project 三类 session 中被真实填充
- [ ] SessionPage / StoryPage 完成第一轮 feature 拆分
- [ ] 业务 DTO 命名收敛到单一风格，测试不接受双读漂移
- [ ] workspace_files 遗留链有明确处置决定
- [ ] 领域负载类型化有盘点清单、错误边界标准和一个改造样板
- [ ] 能清晰回答"项目是否已准备进入稳定化阶段"，并给出可追溯依据
