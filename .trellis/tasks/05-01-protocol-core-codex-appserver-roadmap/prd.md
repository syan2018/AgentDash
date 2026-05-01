# brainstorm: 自维护协议基底规划（Codex App Server）

## Goal

建立一套可长期自维护的“运行时事实主干协议”规划框架：以 Codex App Server Protocol 的 thread/turn/item 语义为主要参考并持续跟踪，同时保留 AgentDash 自主设计与治理空间；ACP 保留为平台外部嵌入与接入面，使内部流转语义统一、外部接入清晰、演进机制可控。

## What I already know

* 当前项目把 ACP 作为统一会话事件与展示协议面，前后端与持久化链路都围绕 `SessionNotification` 构建。
* 用户明确关注点是“协议基底能力”本身，不优先讨论替换成本。
* 用户希望有一套完整规划，既能持续跟踪 Codex App Server Protocol 进展，也能保留内部协议设计自由度。
* Codex App Server 提供了比 ACP 更厚的运行时语义（thread/turn/item、审批、动态工具、fs/watch、rate limits、account/auth 等）。
* Codex 官方支持按本机版本生成 schema（`generate-ts` / `generate-json-schema`），适合做版本对齐与差异追踪。
* 外部生态协议演进速度快，已出现 schema 资产不一致案例，说明“跟踪与治理机制”应成为设计的一部分，而不是附属脚本。
* 用户已明确：首期不考虑多 profile 并行，优先收敛单一运行时事实主干协议；ACP 更偏平台外部嵌入面。
* 用户倾向于直接复用（re-export）Codex 已定义较好的 thread/turn/item 语义。
* 用户对 re-export 深度倾向 `A/B` 混合：很多场景可直接透传；envelope 作为可选治理空间，按需引入而非预设过度封装。
* 用户对 ACP 承载厚语义保持谨慎，倾向把 ACP 作为最小外部嵌入面。
* 用户要求：进入自持协议后，前端绘制数据结构应由 Rust 类型通过 rs-ts 直接导出，减少手写漂移。
* 用户倾向参考 Codex 前端方案：首期直接消费原始事件，暂不额外引入 Render DTO 层。
* ACP facade 首期以最小能力集为主（prompt/stream/cancel/history），不承担内部厚语义。
* 版本治理采用“单线快跟”策略，不引入多环分发复杂度。
* Envelope v0 推荐最小治理位：`trace` + `protocol_version` + `extension_guard`，其余默认透传。
* Experimental 能力默认纳入协议主干（协议层不做额外阻断），避免过度治理。

## Assumptions (temporary)

* 该任务先产出协议治理与演进规划，不直接绑定具体代码迁移排期。
* 首期以“单一运行时主干协议”收敛，不做多 profile 并行治理。
* ACP 在规划内定位为外部嵌入接入面，而非内部主干语义来源。
* 可以大量复用 Codex thread/turn/item 语义，但平台仍需保留自身版本与治理主权。

## Open Questions

* 当前无阻塞开放问题；后续新增问题在 ADR 中逐条登记。

## Requirements (evolving)

* 明确协议愿景与边界：什么属于“内部运行时主干协议”，什么属于“外部嵌入接入面”。
* 明确主干 re-export 策略：默认“能透传就透传”，仅在治理/审计/稳定性需要时加平台 envelope。
* 定义主干协议模型（以 thread/turn/item 为核心）：
  * `Runtime Backbone Semantic Model`（平台内统一语义层）
  * `Backbone Envelope`（平台自有版本、trace、治理字段）
  * `Codex Mapping Rules`（Codex 方法/事件到主干语义的映射规范）
  * Envelope v0 最小字段建议：`trace_id` / `source_backend_id` / `observed_at` / `protocol_version` / `extension_guard`
* 建立 Codex App Server 跟踪机制：
  * 版本采集、schema 拉取、结构 diff、变更分类、影响评估、决策归档。
* 定义内部协议迭代机制：
  * Proposal 模板、评审流程、实验标记、收敛准入、废弃流程。
* 定义 ACP 嵌入面契约：
  * Backbone ↔ ACP Facade 的投影规则
  * 嵌入场景最小能力集（外部 UI 快速接入）
  * 首期最小方法面：`session/new`、`session/prompt`、`session/update`、`session/cancel`、history/read
* 定义前端协议类型产物规范：
  * Backbone / Facade 的绘制所需类型由 Rust 单一来源定义
  * 通过 rs-ts 生成 TS 类型，禁止前端手写镜像结构
  * 首期优先导出原始 Backbone 事件类型，前端直接消费并渲染
  * 当前不规划 Render DTO 层（除非未来出现强制业务需求）
* 定义协议合规与回归验证框架：
  * 语义测试矩阵（生命周期、审批、工具调用、错误、恢复、断流重连）
  * Backbone conformance 与 ACP facade conformance 检查项
* 定义可观测性与治理看板指标：
  * 外部协议变更吞吐
  * Backbone 与 Codex/ACP 投影偏差趋势
  * 未决提案与风险项

## Acceptance Criteria (evolving)

* [ ] 存在一份“协议基底章程（Protocol Charter）”并明确目标、非目标、边界与术语。
* [ ] 存在一份 Runtime Backbone Semantic Model 草案（实体、生命周期、状态机、错误类目）。
* [ ] 存在 Codex 跟踪流水线设计（采集 → diff → 分类 → 评审 → 决策）。
* [ ] 存在 Codex 语义映射策略文档（thread/turn/item 覆盖完整）。
* [ ] 存在 ACP 外部嵌入 facade 契约文档（首期最小方法面明确）。
* [ ] 存在 Envelope v0 字段规范（最小治理位 + 透传规则）。
* [ ] 存在 Rust→TS 类型生成方案（rs-ts）并覆盖前端绘制所需原始事件结构。
* [ ] 存在协议治理流程文档（提案、实验、落地、废弃、版本发布节奏）。
* [ ] 存在可执行的验证计划（conformance 维度、样例输入、验收口径）。

## Definition of Done (team quality bar)

* 规划文档可直接作为后续实现任务拆分输入（非口头共识）。
* 协议术语、分层边界、治理流程在文档中无冲突定义。
* 首批里程碑（M1-M3）可直接进入执行，不依赖额外前置澄清。
* 风险与未知项有明确收敛路径（负责人、触发条件、判定标准）。
* 相关规范链接与上下文引用完整（可追溯到项目内外依据）。

## Out of Scope (explicit)

* 本轮直接改造当前会话执行链路或替换现网协议面。
* 本轮直接落地全部 connector 代码实现。
* 本轮做跨所有执行器的一次性统一适配。
* 本轮做兼容性兜底设计（预研阶段不需要）。

## Research Notes

### 外部协议观察（Codex App Server）

* 优势：运行时语义完整（thread/turn/item + approval + dynamic tools + account + fs）。
* 优势：schema 可按版本生成，适合做“可自动化追踪”。
* 风险：发布节奏与实验特性较快，需治理机制吸收波动。

### 现有通用协议观察（ACP）

* 优势：跨客户端/执行器互操作强，语义最小闭环清晰。
* 局限：高密度 runtime 语义需要依赖扩展约定，原生表达厚度有限。

### Feasible Approaches Here

**Approach A：Codex Wire 直通主干 + ACP Facade**

* How it works: 内部主干几乎直接采用 Codex wire 模型，ACP 仅做外层嵌入投影。
* Pros: 落地快、语义信息损耗最小。
* Cons: 平台对外部 wire 细节耦合高，治理主权偏弱。

**Approach B：Codex 语义重导出（推荐）**

* How it works: thread/turn/item 语义基本沿用 Codex，但主干 envelope、版本和治理字段由平台自持；ACP 作为 facade。
* Pros: 兼顾高语义密度与平台自治；更适合长期自维护。
* Cons: 需要额外定义“语义一致但线格式可控”的边界规则。

**Approach AB：默认透传 + 按需封装（当前倾向）**

* How it works: 默认使用 Codex wire 直通；当出现治理/审计/稳定性需求时，仅对必要字段增加平台 envelope。
* Pros: 保持协议干净、抽象负担低、演进阻力小。
* Cons: 需要明确“何时触发封装”的判定规则，否则容易出现策略漂移。

**Approach C：从零定义主干，再选择性吸收 Codex**

* How it works: 平台先独立定义主干语义，再按需吸收 Codex 概念。
* Pros: 自主性最强。
* Cons: 设计与验证成本最高，容易重复造轮子。

## Recommended Direction

建议采用 **Approach AB（默认透传 + 按需封装）**：

* thread/turn/item 语义优先直接沿用 Codex，减少抽象噪音。
* envelope 作为治理能力保留位，仅在需要时最小引入。
* ACP 作为外部嵌入 facade，专注“可接入性”而非“内部事实来源”。

## Decision (ADR-lite)

**Context**: 需要在“高语义密度”与“长期自主治理”之间取得平衡，并满足“内部语义统一 + 外部嵌入友好”的目标。  
**Decision**: 首期不做多 profile 并行，采用单一 Runtime Backbone；Codex re-export 深度采用 `A/B` 混合（默认可透传，按需封装）；ACP 定位为外部嵌入 facade。  
**Consequences**: 协议保持干净、建模负担更小；但必须补充“封装触发条件”与“必须平台持有字段”规则（v0 先固定最小治理位）。前端默认长期走“原始事件直出”，不引入额外 DTO 抽象。

## Technical Approach

### Protocol Governance Blueprint

* 建立 `Protocol Charter`：
  * 术语字典（Session/Turn/Item/ToolCall/Approval/StopReason/...）
  * 语义边界（Backbone vs Mapping vs Facade）
* 建立 `Change Intake Pipeline`：
  * 输入：Codex 发布与 schema 快照
  * 处理：自动 diff + 语义分类（新增/变更/废弃/破坏）
  * 输出：提案 ticket + 决策记录（ADR）
  * 节奏：发布触发采集 + 每周汇总审阅（hybrid cadence）
* 建立 `Version Policy`：
  * 单线版本策略（single-line fast follow）
  * 不做 LTS/Fast/Experimental 多环治理（必要时再升级）
* 建立 `Experimental Intake Rule`：
  * 协议层默认纳入 experimental 字段/事件
  * 仅在实现层决定是否激活对应能力
* 建立 `Decision Board`：
  * `Adopt` / `Adapt` / `Defer` / `Reject` 四象限策略

### Protocol Artifact Layout（规划）

* `backbone/`：运行时主干语义模型与状态机
* `mappings/codex/`：Codex 方法与事件到主干语义的映射
* `facades/acp/`：ACP 外部嵌入面投影契约
* `bindings/ts/`：通过 rs-ts 生成的前端协议类型绑定
* `conformance/`：主干与 facade 验证用例与断言模板
* `adr/`：协议决策与变更记录

### Milestones（draft）

* **M1 - Charter & Scope Closure**
  * 完成协议章程、术语、边界与治理流程
* **M2 - Core Semantic Draft**
  * 完成核心实体、生命周期、错误分类、扩展点
* **M3 - Codex Tracking Pipeline**
  * 完成 schema 采集、diff 分类、决策模板
* **M4 - Codex Mapping v0**
  * 完成 Codex ↔ Backbone 映射初版
* **M5 - ACP Facade Contract v0**
  * 完成 Backbone ↔ ACP 投影规则与边界定义
* **M6 - Conformance Harness Plan**
  * 完成验证矩阵与回归计划
* **M7 - Rust→TS Type Binding Plan**
  * 完成 rs-ts 产物清单、生成入口与“原始事件直出”前端消费规范

## Technical Notes

* 新建任务目录：`.trellis/tasks/05-01-protocol-core-codex-appserver-roadmap`
* 关键参考：
  * `docs/symphony-spec.md`（Codex app-server 编排落地约束）
  * Codex App Server 官方文档（thread/turn/item、审批、动态工具、fs、account）
  * ACP 官方文档（session/update 最小互操作闭环、扩展机制）
* 现状证据（项目内）：
  * 已存在 `CodexBridgeConnector` 与 `executor_session` 适配链路
  * 执行器依赖树已包含 `codex-app-server-protocol` 相关 crate
