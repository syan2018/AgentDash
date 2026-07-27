# Research: 后端全局模块耦合与稳定边界

- Query: 对整个 Rust 后端做稳定边界审计；覆盖 domain / application / infrastructure / API /
  composition、repository / persistence / migration，以及 workflow / VFS / hooks / capability /
  permission / runtime 等主要域；识别依赖方向反转、共享类型泄漏、重复事实源、跨域状态推进、
  过宽 facade、隐式装配约束和缺失的边界测试，并区分合理内聚与危险变化耦合。
- Scope: internal
- Date: 2026-07-26
- Baseline: `8dc12f7385070f63fd65b5a98247df95edeeae7b` (`main`)
- Observation time: 2026-07-26 01:22 +08:00
- Workspace note: 审计时任务目录内有并行会话生成的未跟踪规划/研究文件；本文只写本文件，
  未修改产品代码、migration 或其他 research 文件。

## Findings

## 1. 结论摘要

后端当前最危险的耦合中心不是 Agent Runtime，而是三条更普遍的结构问题：

1. `agentdash-infrastructure` 同时承担 PostgreSQL adapter、脚本 adapter、Runtime/Host composition、
   Tool adapter 和 Integration selection，导致 Infrastructure 对多个 Application use-case crate
   形成正式反向依赖。
2. `RepositorySet` 虽被规范限定为 composition result，实际已进入大量 route 与 application
   业务函数，成为跨域 service locator；它又使 Project 删除、Shared Library 安装等跨聚合流程
   通过多个独立 repository 逐步推进状态，而没有语义事务边界。
3. API composition 以“先创建 deferred tool 占位符、注册进 Broker、数百行后再 install 真正
   service”的时序完成装配；编译器与现有测试不能证明所有必需 binding 均已安装。

按风险分级：

| ID | 等级 | 根因类别 | 结论 |
| --- | --- | --- | --- |
| B-01 | P0 | persistence / authority | Project 删除跨十余聚合逐条提交，失败会留下部分删除状态；artifact object 甚至没有删除端口 |
| B-02 | P1 | direction / build-time | `agentdash-infrastructure` 是 20+ 正式内部依赖的 omnibus crate，并正式反向依赖多个 application crate |
| B-03 | P1 | facade / locality | `RepositorySet` 泄漏到 32 个 application 文件和 26 个 route 文件，名义 composition boundary 已失效 |
| B-04 | P1 | persistence / authority | AgentTemplate + MCP dependency 安装逐 repo 写入，后段失败会保留部分 dependency 安装 |
| B-05 | P1 | composition / temporal | Runtime Product tools 依赖隐式的 deferred-register-then-install 顺序，且没有最终 completeness gate |
| B-06 | P1 | permission / composition | 文档声明的唯一 `AgentRunPermissionFacade` 没有 production implementation、consumer 或测试，Tool Broker 实际绕过该 seam |
| B-07 | P1 | direction / protocol | Application VFS 直接持有 Relay 的 live `ShellOutputRegistry` 和 Agent tool implementation types |
| B-08 | P2 | shared type / build-time | `agentdash-platform-spi` 大量 re-export `agentdash-agent` 实现类型，18 个 crate 的 SPI hub 不稳定 |
| B-09 | P2 | protocol / shared type | wire contract 直接复用 application-port 类型，生成合同所有权和 application notification seam 粘连 |
| B-10 | P2 | data shape / persistence | `InstalledAssetSource` 的列映射和校验在至少五个 PostgreSQL repository 内重复 |

当前没有证据支持“依赖多就一定错误”。以下高连接边界是合理内聚，应保留其语义而非机械拆分：

- API 是 composition root，允许依赖 application、adapter 与 integration；问题是装配协议不完整和
  全量 bag 向业务层泄漏，不是 API fan-out 本身。
- Workflow executor 的 `LifecycleRun` CAS、稳定 claim 和 durable effect 是同一执行一致性边界；
  其并发/重放测试已能在边界破坏时失败。
- Workflow template bundle 的 procedure + graph 事务，以及 Interaction command receipt/event/state/
  effect intent 事务，是正确的跨表单语义写入端口。
- Runtime Contract / Service API / Wire 目前有独立 crate，物理方向总体比通用 application /
  infrastructure 边界更清楚；本报告不把 Runtime 视为全局中心。

## 2. Rust workspace 与依赖地图

根 `Cargo.toml:2-41` 声明 40 个 workspace member。以下数字经
`cargo metadata --no-deps --format-version 1` 复核，只计算非 dev 的内部 `agentdash-*` 依赖：

| Crate | 正式内部 fan-out | 判断 |
| --- | ---: | --- |
| `agentdash-api` | 29 | composition root，高连接合理，但应只输出已完成装配 |
| `agentdash-infrastructure` | 20 | 非 composition root 名义下的异常高连接，且方向反转 |
| `agentdash-application` | 20 | umbrella application facade，当前仍承载大量业务模块 |
| `agentdash-application-lifecycle` | 11 | Workflow/Lifecycle 编排域，高连接但主要面向 ports/domain |
| `agentdash-application-agentrun` | 13 | Product AgentRun 用例域，高连接，需靠 owned contract 限制 |
| `agentdash-workspace-module` | 10 | 名义“boundary helper”实际组合 domain/contracts/gateways/VFS |
| `agentdash-application-vfs` | 8 | 同时包含 VFS use case、Agent tool adapter 和 Relay streaming |

高 fan-in 稳定候选：

| Crate | 正式内部 fan-in | 当前评价 |
| --- | ---: | --- |
| `agentdash-domain` | 21 | 合理的核心领域依赖 |
| `agentdash-platform-spi` | 19 | 应稳定，但当前 re-export Agent 实现类型，稳定性不足 |
| `agentdash-diagnostics` | 18 | 横切诊断依赖，未发现持有业务事实 |
| `agentdash-agent-runtime-contract` | 15 | owned typed contract，当前方向合理 |
| `agentdash-application-ports` | 14 | application boundary port，但混入 TS/wire shape 需收窄 |

依赖图本身没有 Cargo 循环（Cargo 不允许 package cycle）；实际问题是有向无环图中的层级倒置、
hub 不稳定与业务状态跨多个 port 顺序推进。

## 3. B-01 — P0：Project 删除没有原子持久化边界

### 证据

- `crates/agentdash-application/src/project/management.rs:271-375` 的
  `delete_project_aggregate` 顺序读取并删除 LifecycleRun、Routine、ProjectAgent、
  ExtensionInstallation、ExtensionPackageArtifact、VFS mount、Skill、MCP preset、Workflow graph、
  Agent procedure、Story、Workspace、InlineFile、Settings、StateChange，最后才删除 Project。
- 每一步都是独立 repository await；函数没有 transaction/UoW 输入，也没有 outbox/恢复记录。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_repository.rs:134-147`
  的最终删除只是单独 `DELETE FROM projects`。
- baseline migration 只对少量表建立 `project_id -> projects ON DELETE CASCADE`
  （`crates/agentdash-infrastructure/migrations/0001_init.sql:1106-1109,1122,1159`），不能替代上述
  手工跨聚合清理。
- Artifact metadata 在 `management.rs:301-309` 被删除；但
  `crates/agentdash-platform-spi/src/extension_package.rs:14-24` 的 storage port 只有
  `write_archive_object` / `read_archive_object`，没有 delete，文件系统 archive 会成为无 owner
  的 object。
- Git anchor：删除链主体来自 `c7729b8e7`，说明这是持久存在的显式 application orchestration，
  不是当前并行修改。

### 测试锚点

- `crates/agentdash-application/src/project/management.rs:393-423` 只有 clone name/field 的纯函数测试。
- 仓库搜索没有 Project deletion transaction、故障注入、partial delete 或 artifact object cleanup 测试。
- 现有 migration readiness 只检查表/少量列存在与 retired table 缺席
  (`crates/agentdash-infrastructure/src/migration.rs:186-190`)，不会发现半删除业务状态。

### 风险与爆炸半径

- 任意中间 repository 失败都会让 Project 保留但部分资源已永久删除；重试是否可收敛取决于每个
  repository 的偶然幂等性。
- 删除过程中并发读取可观察到混合状态。
- 新增任何 Project-owned 资源都必须记得修改该函数；遗漏不会产生编译错误。
- 爆炸半径覆盖 Project、Workflow/Lifecycle、Routine、Agent、Shared Library、Extension artifact、
  VFS、Story/Workspace、Settings 与文件 object storage。

### 建议目标边界

建立单一语义端口 `ProjectRetirementPort`（名称可调整）：

- PostgreSQL 实现负责一个数据库事务内清理/级联所有数据库 owner fact；
- object storage 清理由同一 use case 写入 durable cleanup intent/outbox，再由稳定 artifact owner
  执行；不能先删 metadata 再失去 storage ref；
- application 只提交 `retire_project(project_id, expected_revision/actor)`，不枚举 repository；
- migration 直接建立最终 FK/cascade 与 outbox schema，不保留当前手工链兼容层。

必需门禁：在第 N 个数据库步骤故障时整笔回滚；object cleanup receipt lost 时可重放；新 Project-owned
表未纳入 retirement contract 时 architecture/migration test 失败。

## 4. B-02 — P1：Infrastructure 依赖方向反转并成为 omnibus crate

### 证据

`crates/agentdash-infrastructure/Cargo.toml:7-26` 的正式依赖除 domain/SPI 外，还包括：

- `agentdash-application-agentrun`
- `agentdash-application-hooks`
- `agentdash-application-vfs`
- `agentdash-application-workflow`
- `agentdash-workspace-module`
- `agentdash-contracts`
- Runtime、Host、Integration API/Native、LLM provider 等实现/宿主类型

这不是只有 boundary port 的合理 Infrastructure 依赖。production code 直接消费 use-case 类型：

- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_product_projection_repository.rs:1-2`
  用 application-agentrun repository/document 类型实现 PostgreSQL persistence。
- `crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs:30-42`
  同时消费 application-agentrun frame/runtime port、Runtime 和 Host。
- `crates/agentdash-infrastructure/src/runtime_tool_executors.rs:6-29`
  同时映射 Runtime executor、Application AgentRun task、Application VFS、Contracts 与 Platform SPI。
- `crates/agentdash-infrastructure/src/complete_agent_product_hook_handler.rs:3-9`
  把 Host callback、AgentRun binding repo 与 Application Hook provider 组合在 Infrastructure。
- `crates/agentdash-infrastructure/src/production_complete_agent_selection.rs:3-16`
  把 Host live catalog、Product profile、Native integration 与 LLM resolver 组合在 Infrastructure。

`.trellis/spec/backend/architecture.md` 的 invariant 明确要求
Interface -> Application -> Domain/SPI，Infrastructure 实现 Domain/SPI persistence port，不依赖
application 编排层；当前实现与该目标相反。

Git anchors 表明倒置是近期多次横向叠加，而非一个局部历史残留：

- Application AgentRun dependency: `312976a5d`
- VFS dependency: `b0e92a434`
- Workflow dependency: `679a6802e`
- Hooks dependency: `2e653ab1e`
- Contracts dependency: `c95f44b8c`

### 测试锚点

- `scripts/lib/quality-gates.js:11-34,116-124` 的 PR quick 只有 migration guard、
  test-support guard 和普通 `cargo check`，没有 package dependency allowlist。
- `scripts/check-test-support-boundaries.js:14-39` 只守护 test fake 的文件位置，不检查生产层级。
- 因此新增 `infrastructure -> application-*` 依赖会通过现有编译/CI。

### 风险与爆炸半径

- Product use-case 类型变化会重编译/修改 persistence 和所有 runtime composition adapter。
- PostgreSQL repository crate 无法在不编译 Runtime/Host/Integration/VFS/LLM 等大部分后端的情况下
  独立验证。
- “Infrastructure” 这个名字掩盖了至少三类不同变化原因，促使新 adapter 继续放入同一 crate。
- 爆炸半径向上到 API（29 个 production 内部依赖），横向到多个 application crate，向下到 domain/schema。

### 建议目标边界

直接拆成最终责任边界，不保留 umbrella compatibility facade：

1. `agentdash-persistence-postgres`：只依赖 domain、低层 typed application port（确实属于 application
   boundary 的 port）和 SQL/serialization helper。
2. `agentdash-script-adapters`：Rhai Hook/Workflow/Operation evaluator，实现稳定 SPI。
3. `agentdash-runtime-composition`：Product/Runtime/Host/Integration adapter；由 API composition root
   构造，允许依赖相应 use-case contract，但不与 persistence 共 crate。
4. `agentdash-runtime-tool-adapters`：Runtime tool definition/executor 到 Product/VFS/Task 的映射。

新增 `cargo metadata` dependency rule：persistence 禁止依赖 `agentdash-application-*` use-case crate、
API、Host、Integration；composition adapter 允许的边必须逐项列出。

## 5. B-03 — P1：RepositorySet 已从 bootstrap result 退化为 service locator

### 证据

- `crates/agentdash-application/src/repository_set.rs:43-84` 暴露约 40 个公开 repository 字段，覆盖
  几乎全部业务域。
- 同文件 `:86-109` 再把全量 set 投影成 Workflow/SharedLibrary 子 set，说明 caller 仍需知道其他
  域的 repository packaging。
- 定向搜索结果：
  - 32 个 `agentdash-application/src` 文件引用 `RepositorySet`；
  - 26 个 API route 文件直接访问 `state.repos`；
  - application 中 `RepositorySet` 字段/函数签名约 91 处。
- 示例：
  - `crates/agentdash-application/src/project/management.rs:55-134` 的普通 create/load/update/delete
    函数全部接收全量 set。
  - `crates/agentdash-application/src/routine/executor.rs:66-80` 持有全量 set。
  - `crates/agentdash-application/src/frame_construction/mod.rs:56-69` 的 service/deps 持有全量 set。
  - `crates/agentdash-api/src/routes/backends.rs:103-109,189-195` route 直接组合多个 repository 查询。
  - `crates/agentdash-api/src/routes/agent_run_workspace.rs:100-109` route 自己遍历 lineage/agent repository。
- `.trellis/spec/backend/repository-pattern.md:18,46-48` 和
  `.trellis/spec/backend/architecture.md:62,94` 明确规定：RepositorySet 只用于 bootstrap/AppState，
  业务 route helper/application constructor 必须接收具名 use-case deps。当前是已证实的 spec drift。

### Git 与测试锚点

- `RepositorySet` 初始来自 `ecc616066`，随后每个新域持续添加公开字段；其 git blame 呈现典型
  “新增功能就扩大全局 bag”的变化模式。
- 普通 unit test 能用全量 fake set 继续工作，不会因用例读取了新 repository 而暴露依赖扩大。
- 当前没有“业务模块不得引用 RepositorySet”静态 guard。

### 风险与爆炸半径

- constructor 不表达真实依赖；review/test 不能判断某用例是否跨了新 aggregate。
- 任何 repository trait 变更会触发全量 fixture/AppState/wiring 变化。
- route 可绕过 application use case，导致授权、事务、通知、outbox 等规则分散。
- 它直接促成 B-01/B-04：跨域状态推进看起来只是多访问几个公开字段。

### 建议目标边界

- `RepositorySet` 只保留在 `bootstrap/repositories.rs` 的私有 output；`AppState` 不向 route 公布该 bag。
- 每个 use case 定义最小 deps，例如 `DeleteProjectDeps` 最终应只包含一个
  `ProjectRetirementPort`，而不是十余 repository。
- route 只依赖具名 command/query service；鉴权后不直接读 repository。
- 添加 source guard：除 bootstrap/composition 指定文件外，禁止 production code 出现
  `RepositorySet` / `state.repos`。

## 6. B-04 — P1：Shared Library AgentTemplate 安装会产生部分提交

### 证据

- `crates/agentdash-application-shared-library/src/repository_set.rs:17-29` 暴露 11 个跨域 repository。
- `crates/agentdash-application-shared-library/src/install.rs:402-449`：
  1. 先解析所有 MCP dependency plan；
  2. `:417-429` 逐个调用 `upsert_mcp_preset` 写入 Project MCP Preset；
  3. `:431-449` 最后才构造并写入 ProjectAgent。
- `upsert_mcp_preset` 在 `:936-963` 直接 create/update 独立 repository；
  `upsert_project_agent_template` 在 `:452-478` 再操作另一个 repository。
- 如果第二个 dependency 或 ProjectAgent 写入失败，前面 preset 已提交，没有 rollback/receipt。
- 对比正确边界：WorkflowTemplate 使用
  `WorkflowTemplateInstallRepository::install_workflow_template_bundle`
  (`install.rs:966-994`)；PostgreSQL 有事务测试
  `crates/agentdash-infrastructure/src/persistence/postgres/workflow_repository.rs:1141`。
- `.trellis/spec/backend/shared-library.md:3,68-70` 把安装称为事务，并明确 Workflow bundle
  必须原子提交；Agent dependency plan 同样是一个用户可见安装意图，但当前没有等价 semantic port。

Git anchor：MCP dependency 写链来自 `e4e708097`，direction merge 来自 `d62ce98c0`；这是一条当前
production feature，而非未挂载代码。

### 测试锚点

- `install.rs:1135-1392` 的测试主要覆盖纯解析、参数 merge、visibility/merge 规则。
- 没有故障注入测试证明第 N 个 MCP preset 或 ProjectAgent 写失败时 Project 资源保持不变。
- Workflow bundle 的事务测试证明仓库已有正确实现模式，缺口是边界覆盖不一致。

### 风险与爆炸半径

- API 返回安装失败但 Project 已新增/覆盖部分 MCP Preset；重试可能遇到 key conflict 或覆盖用户已有值。
- Agent config 中 dependency directive 与实际 preset 集合可能不一致。
- 爆炸半径覆盖 Shared Library、ProjectAgent、MCP Preset、capability directive、source-status。

### 建议目标边界

定义 `ProjectAssetInstallTransaction`/`AgentTemplateInstallPort`，一次提交：

- dependency MCP preset create/update；
- ProjectAgent create/update；
- 所有 `InstalledAssetSource`；
- installation receipt/idempotency identity。

Application 负责完整 plan/validation，PostgreSQL adapter 负责事务应用；测试必须覆盖 overwrite、
第二 dependency 失败、Agent 写失败和 receipt lost 重试。项目未上线，直接替换当前逐 repo 写链。

## 7. B-05 — P1：Runtime Product tool 装配依赖隐式时序

### 证据

- `crates/agentdash-infrastructure/src/runtime_tool_executors.rs:109-140` 定义
  `DeferredProductRuntimeToolService`，内部 `OnceLock` 允许后续 install。
- 未 install 时不是 bootstrap 失败，而是在实际工具调用时返回
  `product_runtime_tool_not_installed` (`:142-163`)。
- API 先创建 7 个 deferred service
  (`crates/agentdash-api/src/app_state.rs:320-347`)，立即将其加入最终 runtime catalog/Broker
  (`:348-376`)；真正 service 到 `:646-707`、`:741-752` 才安装。
- `is_installed()` 仅定义于 `runtime_tool_executors.rs:137-139`，全仓没有 caller；AppState 在
  `app_state.rs:761-824` 发布并启动 background workers 前不做 completeness validation。
- `AppState::new_with_integrations` 从 `:247` 到 `:824` 以约 578 行局部变量隐式表达依赖顺序，
  任意提前 return/reorder/新增 tool 都可能留下运行期缺口。

Git anchors 显示各 tool 安装分散在多个 feature commit：

- Workspace Module: `8dd149365`
- Companion: `2216364de`
- Lifecycle: `e995a82e0`/相邻 runtime work

### 测试锚点

- `crates/agentdash-api/src/app_state.rs:828-916` 的测试只覆盖 env 解析和可选 Complete Agent 错误分类，
  没有构造完整 AppState 并断言所有 required tool ready 的 composition test。
- 全仓没有 `is_installed()` consumer，也没有删除任一 `.install(...)` 后必失败的测试。

### 风险与爆炸半径

- 编译、cargo check、Broker catalog 测试都可通过，但特定 tool 第一次调用才失败。
- schema 在 placeholder 创建时固定，service 在后面创建；二者只用 `kind` 校验，不验证 schema/
  capability/effect 一致性。
- 爆炸半径覆盖 AppState、Tool Broker、Lifecycle、Companion、Workspace Module、Workflow 和 Runtime Host。

### 建议目标边界

- 每个 bootstrap slice 返回完整 `RuntimeToolContribution { definition, executor }`，Broker 只接受已完成
  contribution；不要把半初始化 object 放入 production catalog。
- 如果循环装配客观存在，使用显式 `CompositionBuilder` 两阶段协议：declare -> bind -> validate/freeze；
  `freeze()` 返回不可变完整 catalog，缺 binding 在启动期失败。
- composition contract test 枚举 required `ProductRuntimeToolKind`，断言 definition/executor/deps 都 ready；
  删除任一 binding 时测试必须失败。

## 8. B-06 — P1：Permission facade 是未接线的名义边界

### 证据

- `crates/agentdash-application-ports/src/agent_run_permission.rs:5-39` 定义 request/decision/error/facade。
- 全仓对 `AgentRunPermissionFacade`、`AgentRunPermissionDecision` 的生产引用只有该定义和
  `lib.rs` module export；没有 `AllowAllAgentRunPermissionFacade` implementation。
- 实际 Tool Broker 只调用 `RuntimeToolAuthorizationPort`
  (`crates/agentdash-agent-runtime/src/platform_tool_broker.rs:207-227,319-365`)。
- production authorizer 在
  `crates/agentdash-infrastructure/src/runtime_tool_authorization.rs:31-97`
  根据 Product binding + applied surface 生成 capability/VFS/task grant；没有调用 permission facade。
- `.trellis/spec/backend/permission/architecture.md` 却声明 facade 是当前唯一入口、当前 production
  implementation 为 allow-all，并要求 Tool Broker 覆盖 Allowed/Denied/PendingApproval。

### 测试锚点

- `platform_tool_broker.rs:369-519` 只测试 Runtime authorization allow/deny。
- 全仓没有 Permission facade 单测，也没有 Broker permission decision 的三个分支。
- 因而 spec 中“未来在 facade 内增加 LifecycleRun-backed decision，不改变 Broker”的扩展承诺不成立。

### 风险与爆炸半径

当前产品事实明确是 allow-all，所以没有证据证明正在发生权限拒绝错误；风险是稳定边界虚假：

- 后续实现 Grant 时，开发者可能只实现 dead facade，而 Tool Broker 仍执行副作用；
- 或把 Grant 再塞进 RuntimeToolAuthorizationPort，造成 capability/resource admission 与 user permission
  两个概念继续混合。

爆炸半径覆盖 Permission、Capability、Applied Surface、Tool Broker、RuntimeInteraction 和 future Grant
persistence。

### 建议目标边界

先做二选一的最终决策，不保留双 seam：

1. 若当前确实需要稳定 permission 扩展点：把 facade 接到 Broker side effect 前，提供真实 allow-all
   implementation 和三分支测试；
2. 若当前不需要：删除 dead facade/spec 承诺，等产品语义明确后以一个 owner contract 落地。

现有设计文档已选择方案 1，因此实现验收应以 production composition 可达为准。

## 9. B-07 — P1：Application VFS 泄漏 Relay live registry 和 Agent tool 类型

### 证据

- `crates/agentdash-application-vfs/src/tools/fs/shell.rs:72-84` 的 `ShellExecExecutor` 同时持有
  `VfsService`、`agentdash_relay::ShellOutputRegistry`、terminal registry、materialization、
  Runtime identity、auth identity 与 capability state。
- `:107-112` 的 public builder 直接接收 Relay concrete registry。
- `crates/agentdash-application-vfs/src/runtime_tools.rs:94-130` 和
  `tools/factory.rs:18-47` 继续传播同一 concrete registry。
- `crates/agentdash-relay/src/shell_output_registry.rs:8-42` 明确它是 WebSocket
  `EventToolShellOutput` 的 process-local call_id -> channel 路由表，不是 VFS 领域事实或 application port。
- Application VFS 的 tool modules 还直接返回/消费 `agentdash_agent::AgentToolResult`,
  `AgentToolError`, `ToolProtocolProjector`：
  `tools/mod.rs:23-55`、`tools/fs/read.rs:301-302`、`tools/fs/shell.rs:787-801`。

Git anchor：Relay registry dependency 从旧 application VFS 搬迁而来（`82387148d`），crate split
没有改变真实边界；terminal registry 后加于 `ed29ec87a`。

### 风险与爆炸半径

- Relay event/payload/stream registry 变化穿透 VFS tool execution。
- Agent protocol projection 变化要求修改 VFS core/tool modules。
- VFS 无法在不依赖 Relay/Agent 实现的情况下作为稳定文件能力独立验证。
- Shell execution 同时解释 VFS policy、Runtime identity、Relay streaming 与 Agent presentation，
  变化原因过多。

### 建议目标边界

- 保留 `VfsService`、address/policy/materialization 为 application-vfs core。
- 在 application port 定义 protocol-neutral `ShellOutputSubscription` /
  `ShellTerminalObservationPort`；API Relay adapter 实现该 port。
- Agent tool/result/projector 映射移动到 runtime tool adapter crate；VFS core 返回 typed VFS outcome。
- 用 contract test 验证 output chunk 顺序、unsubscribe、disconnect 和 truncation，而不是让 VFS import
  WebSocket registry。

## 10. B-08 — P2：Platform SPI re-export Agent implementation，稳定 hub 被污染

### 证据

- `crates/agentdash-platform-spi/Cargo.toml:8-11` 正式依赖
  `agentdash-agent-runtime-contract`、`agentdash-agent`、`agentdash-domain`。
- `crates/agentdash-platform-spi/src/lib.rs:8-30` 大量 `pub use agentdash_agent::*`，包含 runtime
  delegates、compaction、tool、message、projection 等实现侧类型。
- 注释明确写“保持外部 API 不变”，属于兼容 facade；当前项目未上线，不需要继续保留此类 umbrella
  re-export。
- SPI fan-in 为 18；任何 `agentdash-agent` public type 调整都可能传播给 Application、VFS、Hooks、
  LLM Provider、API 等 consumers。
- Git anchor：re-export 历史来自旧 `agentdash-spi` / connector contract，当前路径迁移 commit
  `e781a1361` 没有消除兼容面。

### 风险与建议

这是 build-time/data-shape coupling，未发现第二份业务事实，故为 P2 而非 P0/P1。

目标：

- Agent loop/tool/hook contract 由 dependency-light contract crate 拥有；
- Platform SPI 只表达平台提供能力，不 re-export Agent implementation；
- consumer 从真实 owner 导入；
- 用 cargo metadata allowlist 保护 `platform-spi` 不依赖 `agentdash-agent` implementation crate。

## 11. B-09 — P2：Application notification type 同时成为 wire contract

### 证据

- `crates/agentdash-application-ports/src/project_projection_notification.rs:8-47`
  的 `ControlPlaneProjectionChanged` / reason enum 同时 derive serde、JsonSchema、TS。
- 同文件 `:49-88` 又定义内部 UUID/RuntimeThreadId invalidation 与 application notification port。
- `crates/agentdash-contracts/src/project/contract.rs:7,73-96` 直接 import 该 application-port type，
  嵌入 `ProjectEventStreamEnvelope`，因此 wire/generator crate 依赖 application seam。
- Git anchor：这次粘连来自 `584b98b00`。

### 风险与建议

内部 notification 语义、Runtime coordinate 与前端 stream wire shape 变化被同一文件/类型绑定。
当前只有一个 producer shape，未发现重复事实，因此为 P2。

目标：

- 将稳定 wire enum/DTO 放在独立、dependency-light contract owner；
- application invalidation 保持 UUID/typed coordinate；
- API mapper 是唯一 internal -> wire 转换点；
- generated contract drift test继续守 wire，但再加 application-to-wire exhaustive mapper test。

## 12. B-10 — P2：InstalledAssetSource persistence mapper 重复

### 证据

以下 repository 各自重复
`installed_library_asset_id/source_ref/source_version/source_digest/installed_at` 提取和
`parse_installed_source` 必填校验：

- `persistence/postgres/agent_repository.rs:197-252`
- `persistence/postgres/mcp_preset_repository.rs:188-235`
- `persistence/postgres/project_extension_installation_repository.rs:248-340`
- `persistence/postgres/skill_asset_repository.rs:505-551`
- `persistence/postgres/workflow_repository.rs:884-930`

字段错误字符串与 UUID parse 规则也被复制。Project VFS Mount 则使用 JSONB
(`project_vfs_mount_repository.rs:31,48-50`)，同一个安装来源概念存在两种持久化 shape。

### 风险与建议

新增 InstalledAssetSource 字段或收紧 invariant 时至少修改五个 mapper、migration 多张表和各自测试，
很容易只更新部分资产种类。当前事实仍由各 Project resource 持有，没有发现竞争 owner，故为 P2。

目标：

- 在 PostgreSQL adapter 内建立一个共享 `InstalledAssetSourceColumns` row/bind codec；
- 统一所有 Project asset 的最终 schema shape；
- table-driven round-trip test 枚举 ProjectAgent/MCP/Skill/Workflow/Extension/VFS Mount。

## 13. 主要域边界审计矩阵

| 域 | Canonical owner / 当前正确部分 | 当前危险耦合 | 评级 |
| --- | --- | --- | --- |
| Project / Backend / Workspace | Domain repositories；Project auth rule 在 domain；Backend 跨聚合 auth 在 application | Project delete 逐 repo；routes 直接读 repo；RepositorySet 贯穿 workspace/backend use case | P0/P1 |
| Story / Task | Story + StateChange repository；API 有 contract DTO | route/application 仍接收全量 RepositorySet；Project deletion 手工枚举 | P1 |
| Shared Library / Asset | LibraryAsset 与 InstalledAssetSource owner 清楚；Workflow bundle 有事务 | Agent+MCP dependency 非事务；InstalledAssetSource mapper 重复 | P1/P2 |
| Workflow / Lifecycle | LifecycleRun CAS、durable executor effect、Workflow bundle transaction | Lifecycle composition 依赖 AppState 大范围 wiring；但未发现竞争 owner | 合理内聚 + composition P1 |
| Routine | Routine/Execution repository 清楚 | executor 持有全量 RepositorySet，容易跨域扩张 | P1 |
| Interaction / Canvas | InteractionCommandTransactionPort 和单 Postgres repository 收束 command/event/state/effect | Workspace Module helper 同时依赖 contracts/gateways/VFS，变化面偏宽 | 当前核心边界合理，helper P2 |
| VFS | VFS Service/address/policy/materialization 是明确能力域 | Relay live registry 和 Agent tool presentation 进入 application-vfs | P1 |
| Hooks | Application Hooks 主要依赖 domain/application ports/SPI；Rhai 是 adapter | Product CompleteAgent Hook handler 被放入 omnibus Infrastructure | B-02 的 P1 |
| Capability | CapabilityResolver 声明 Frame/Applied Surface 权威，Product authorizer验证 binding digest | Platform SPI 兼容 re-export；Capability 与 permission seam 实际未串联 | P1/P2 |
| Permission | 目标 owner 是 AgentRun facade / RuntimeInteraction | facade 完全未接线，当前只有 Runtime resource authorization | P1 |
| Agent Runtime / Host / Driver | Contract/Service API/Wire 独立 crate；Runtime in-memory，Host live route | composition/provisioning/tool adapter 被塞入 Infrastructure；AppState 时序装配 | P1 |
| Persistence / Migration | 单 baseline migration适合未上线项目；Interaction/Workflow 有事务测试 | Project retirement、Shared Library compound install缺少同等级 semantic transaction | P0/P1 |
| API / MCP / Relay / Local | API 是合法 composition root；Relay typed protocol单独 crate | AppState 公布全量 repo/service bags；VFS依赖 Relay concrete registry | P1 |

## 14. 合理内聚：不要机械拆除的边界

### 14.1 Workflow executor 的 CAS + durable effect

- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:1819`
  `concurrent_drains_commit_one_atomic_agent_start` 守护并发 drain 只产生一次 Agent start。
- 同文件 `:1847` 有 stale CAS test，`:1876` 有 accepted Product effect 在 CAS failure 后稳定 claim replay，
  `:1923-2070` 覆盖 function effect receipt loss / inspect-only recovery。

这类连接属于同一执行一致性边界；应以 typed reducer/effect repository继续内聚，不应按文件行数拆散。

### 14.2 Workflow template transaction

- `install.rs:966-994` 通过单个 semantic port提交 procedure + graph。
- `workflow_repository.rs:1141` 有 PostgreSQL transaction/overwrite/version 测试。

这是 B-04 的目标范式，而不是“repository 太大”的坏例子。

### 14.3 Interaction command transaction

- `RepositorySet` 虽分别暴露 Interaction 多个读 port，但 bootstrap
  `crates/agentdash-api/src/bootstrap/repositories.rs:82-83,174-180` 用同一
  `PostgresInteractionRepository` 实现 definition/instance/command transaction/event/presentation。
- `crates/agentdash-infrastructure/src/persistence/postgres/interaction_repository.rs:1185`
  还有 migration/repository required columns 同步测试。

Command receipt/event/state/effect intent 必须共事务，这种“一个 concrete repository 实现多个窄 read/
write port”是合理内聚。问题只在全量 RepositorySet 向所有 caller 公布这些 port。

### 14.4 API composition root 的高 fan-out

API production fan-out 29 本身不是发现。Composition root 必须看见 concrete adapter。整改重点是：

- 把 B-02 中伪装成 Infrastructure 的 composition 移到明确 composition package；
- bootstrap 返回完整、已验证 output；
- AppState 给 route 暴露 use-case/query service，而不是 repository/service bag。

## 15. 自动门禁建议

按优先级：

1. **Cargo dependency architecture gate**
   - 解析 `cargo metadata --no-deps`；
   - persistence 禁止依赖 application use-case/API/Host/Integration；
   - domain 禁止依赖 application/infrastructure/interface；
   - platform-spi 禁止依赖 Agent implementation；
   - 注册到 `pr_quick`，而不是只在 heavy check。
2. **RepositorySet source gate**
   - allowlist 仅 bootstrap/composition；
   - route 禁止 `state.repos`；
   - application service禁止参数/字段 `RepositorySet`。
3. **Composition completeness test**
   - 枚举所有 required Runtime/Product tools、Hook callback、Operation gateway、provider；
   - freeze 后不得有 deferred/uninstalled binding。
4. **Compound mutation failure tests**
   - Project retirement 每一步故障注入；
   - AgentTemplate dependency 第 N 步、Agent 写入失败；
   - object cleanup receipt loss/replay。
5. **Shared persistence codec tests**
   - `InstalledAssetSource` table-driven round trip；
   - schema column list与 codec字段同步。
6. **Permission reachability test**
   - Tool side effect 前必须经过 resource authorization 和 permission decision；
   - Allowed/Denied/PendingApproval 均可从 production composition 到达。

## 16. 建议整改顺序与可拆任务

### Wave 0：先建立失败门禁

1. `backend-cargo-dependency-architecture-guard`
2. `backend-repository-set-boundary-guard`
3. `runtime-composition-completeness-characterization`
4. `project-retirement-and-shared-install-failure-characterization`

### Wave 1：先修 authority / transaction

1. `project-retirement-semantic-port-and-object-cleanup`
2. `shared-library-agent-template-install-transaction`
3. `agentrun-permission-production-seam`

### Wave 2：恢复依赖方向

1. `split-postgres-persistence-from-runtime-composition`
2. `split-vfs-core-from-relay-and-agent-tool-adapters`
3. `narrow-platform-spi-agent-contract`

### Wave 3：收窄 facade / composition

1. `remove-repository-set-from-application-use-cases`
2. `replace-appstate-service-bag-with-domain-bootstrap-outputs`
3. `replace-deferred-runtime-tools-with-validated-composition-builder`

### Wave 4：消除局部数据形状耦合

1. `control-plane-notification-wire-contract-ownership`
2. `installed-asset-source-postgres-codec`

依赖关系：Wave 0 先固定行为；Project/Shared Library transaction 不应等待大规模 crate split；crate split
完成后再收缩 AppState/RepositorySet，避免把当前倒置原样搬到新 facade。

## Files Found

- `Cargo.toml` — Rust workspace member 与共享内部依赖清单。
- `crates/agentdash-infrastructure/Cargo.toml` — Infrastructure 正式反向依赖证据。
- `crates/agentdash-infrastructure/src/**` — PostgreSQL、Runtime/Host composition、Tool/Hook/script adapter
  混合实现。
- `crates/agentdash-application/src/repository_set.rs` — 全局 repository bag。
- `crates/agentdash-api/src/app_state.rs` — deferred tool 与全局 service composition 顺序。
- `crates/agentdash-application/src/project/management.rs` — Project 跨聚合非事务删除。
- `crates/agentdash-application-shared-library/src/install.rs` — Shared Library compound install。
- `crates/agentdash-application-vfs/src/**` — VFS core、Relay registry、Agent tool adapter 混合。
- `crates/agentdash-application-ports/src/agent_run_permission.rs` — 未接线 permission facade。
- `crates/agentdash-agent-runtime/src/platform_tool_broker.rs` — 实际 Runtime tool admission 调用链。
- `crates/agentdash-platform-spi/src/lib.rs` — Agent/domain compatibility re-export hub。
- `crates/agentdash-contracts/src/project/contract.rs` — application-port 类型进入 wire envelope。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` — 最终 PostgreSQL baseline、FK 与 owner schema。
- `scripts/lib/quality-gates.js` — 当前 CI gate 组成。
- `.trellis/spec/backend/*.md` 及各 domain architecture — 目标依赖、repository、transaction 与 owner
  契约。

## Code Patterns

- **危险：全量依赖 bag** — `repository_set.rs:43-84`，caller 可随意扩张跨域读取/写入。
- **危险：逐 repo compound mutation** — `project/management.rs:271-375`、
  `shared-library/install.rs:417-449`。
- **危险：半初始化 registry member** — `runtime_tool_executors.rs:109-163`。
- **危险：application 依赖 concrete transport registry** —
  `application-vfs/tools/fs/shell.rs:72-112`。
- **危险：SPI compatibility re-export** — `platform-spi/src/lib.rs:8-30`。
- **合理：semantic transaction port** — `shared-library/install.rs:987-994`。
- **合理：CAS + stable effect identity** —
  `application-workflow/orchestration/executor_launcher.rs:1819-1913`。
- **合理：一个 concrete transaction repository实现多个窄 Interaction ports** —
  `api/bootstrap/repositories.rs:82-83,174-180`。

## External References

- 无。本研究只使用当前仓库代码、migration、tests、git blame/log 与 `.trellis/spec`；未使用联网资料。

## Related Specs

- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/directory-structure.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/domain-payload-typing.md`
- `.trellis/spec/backend/shared-library.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/hooks/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/interaction/architecture.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`

## Caveats / Not Found

- 本次是静态 production reachability、migration、test 与 git 审计，没有运行全量 cargo test；按任务
  planning 约束也不需要为研究运行无关全量构建。
- fan-in/fan-out 只统计 Cargo manifest 中正式 `agentdash-*` dependencies，不把 dev-dependency、
  feature 条件、源码局部使用次数当作架构质量分数；风险判断另有 production code 证据。
- 未发现 Cargo package cycle；“循环”风险主要表现为 composition 时序和跨 owner 状态推进，而非
  Cargo 图的字面环。
- 没有发现当前 production 中持久化的长期 Permission Grant；B-06 的当前行为仍是 allow-all。
  结论是 facade/spec 与真实执行链不一致，而不是声称当前已有 Grant 被错误拒绝。
- Workspace Module helper 的 fan-out 很高，但其核心 Interaction command authority 已由
  Interaction repository/transaction 收束；目前证据不足以把 Workspace Module 判为独立 P1，
  故只在矩阵中标为 P2 候选。
- `agentdash-infrastructure` 的 `application-ports` 依赖可能是实现 application boundary port 的合理边；
  B-02 针对的是它对 application use-case crates、Host/Integration 以及 composition 类型的混合正式依赖，
  不是要求 Infrastructure 完全不能实现 application-defined port。
