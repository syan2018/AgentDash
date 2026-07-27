# Research: 全仓依赖结构、演化耦合与稳定边界

- Query: 盘点 workspace/package graph、crate/package 依赖方向、循环或超宽 hub、共享/生成契约、
  migration 分布、测试装配，并结合已落盘历史判断 co-change/churn 热点；区分合理 composition 与
  稳定边界失效。
- Scope: internal（代码、manifest、Trellis task/session memory）；Git 定量历史由主审计流补齐。
- Date: 2026-07-26

## Findings

### 1. 方法、口径与可复现命令

#### 1.1 静态依赖图

审计读取了根 `Cargo.toml` 声明的 40 个 workspace crate
（`Cargo.toml:1-45`）与 pnpm workspace 中 6 个正式 package
（`pnpm-workspace.yaml:1-3`，examples 不计入正式 package 数）。

Rust 图算法：

1. 枚举 `crates/*/Cargo.toml`；
2. 同时识别 `agentdash-foo = ...` 与 `agentdash-foo.workspace = true` 两种声明；
3. 将 `[dependencies]` / `[build-dependencies]` 作为 production edge；
4. 将 `[dev-dependencies]` 单独作为 test edge；
5. 分别对 production graph 与 production+test graph 运行 Tarjan SCC；
6. fan-in/fan-out 只作候选筛选，最终风险按依赖语义、owner 与变化传播判断。

PowerShell/Node 等价复现命令与检查：

```powershell
rg --files -g 'Cargo.toml' -g 'package.json' -g 'pnpm-workspace.yaml'
rg -n '^agentdash-application' crates -g 'Cargo.toml'
rg -n '^use agentdash_application|agentdash_application_' crates/agentdash-infrastructure/src
rg -n '^use agentdash_agent::|agentdash_agent::' crates/agentdash-platform-spi/src --glob '*.rs'
rg -n '^CREATE TABLE' crates/agentdash-infrastructure/migrations/0001_init.sql
rg --files crates tests scripts | Where-Object {
  $_ -match '(^|[\\/])tests?[\\/]|\.test\.|\.spec\.'
}
rg -n 'packages[/\\]app-web[/\\]src[/\\]generated|generated[/\\]' crates scripts package.json
```

本次生成的临时分析脚本已删除，research 目录只保留本报告。

#### 1.2 规模数据

当前 Rust source footprint（只用于识别需要语义核验的候选，不直接作为风险结论）：

| crate | Rust 文件 | 约行数 | 解释 |
| --- | ---: | ---: | --- |
| `agentdash-application` | 126 | 44,955 | 旧聚合应用层与已拆分 application crate 并存 |
| `agentdash-api` | 104 | 35,138 | HTTP/relay/bootstrap composition root |
| `agentdash-integration-codex` | 9 | 32,126 | 大量 vendor/codegen 体积，不等于业务 hub |
| `agentdash-agent-protocol` | 18 | 27,437 | canonical/vendor protocol 类型 |
| `agentdash-infrastructure` | 66 | 25,138 | persistence 与 production composition 混合 |
| `agentdash-domain` | 115 | 20,862 | 合理的底层 domain fan-in |
| `agentdash-local` | 50 | 18,505 | 本机 runtime composition |

前端当前 `app-web/src`：

| area | 文件 | 约行数 |
| --- | ---: | ---: |
| `features` | 376 | 63,359 |
| `generated` | 273 | 9,455 |
| `pages` | 15 | 4,668 |
| `services` | 48 | 4,387 |
| `stores` | 17 | 3,995 |

`generated` 的 273 个文件中，236 个属于 `codex-app-server-protocol` vendor closure；不能把
文件数量直接解释为 273 个自有协议。

#### 1.3 历史证据

研究角色禁止运行 Git 命令。因此本报告没有直接执行 `git log`、`git diff-tree` 或按 commit
重算 Jaccard/co-change/churn。历史证据来自：

- 已归档 Trellis task；
- `trellis mem search/context/extract` 返回的本地会话；
- 会话内已记录的 commit hash。

使用过的 memory 命令：

```powershell
trellis mem search "回归" --cwd F:/Projects/AgentDash --since 2026-04-01
trellis mem search "连锁" --cwd F:/Projects/AgentDash --since 2026-04-01
trellis mem context 019ec49e-c2b --grep "连锁" --turns 4 --around 4
trellis mem context 019d4925-478 --grep "embedded PostgreSQL" --turns 4 --around 4
trellis mem context 019f14a3-09d --grep "重复推断" --turns 4 --around 4
trellis mem extract 019ec49e-c2b --grep "commit"
```

所以后文的 co-change 是“有 commit/task 锚点的定性共变”，不是完整统计。主报告若需要 Top-N
commit pair、时间窗 churn、Jaccard 或 lift，必须由允许读 Git history 的主审计流补齐，不能把本报告
的定性样本冒充全量数据。

---

### 2. 仓库模块与边界地图

#### 2.1 Rust production DAG

根 workspace 有 40 个 crate（`Cargo.toml:3-45`）。按当前 production manifest edge 运行 SCC：

- **production SCC 数量：0**；
- 因而当前不存在 Cargo 层的生产循环依赖；
- 这只证明编译图是 DAG，不证明依赖方向正确。

production fan-out 前列：

| crate | internal production fan-out | 语义判断 |
| --- | ---: | --- |
| `agentdash-api` | 29 | composition root 为主，数量本身合理；具体类型向 route 泄漏是问题 |
| `agentdash-application` | 20 | 聚合 facade 与真实业务实现混合，变化隔离失败 |
| `agentdash-infrastructure` | 20 | persistence crate 吸收 application/integration/composition，方向失败 |
| `agentdash-application-agentrun` | 13 | 业务纵向切片偏宽；本报告不以 Runtime 为中心展开 |
| `agentdash-local` | 13 | Local composition 多数合理，但直接依赖聚合 application 扩大变化面 |
| `agentdash-application-lifecycle` | 11 | 纵向业务切片，需与 domain owner 一起判断 |

production fan-in 前列：

| crate | internal production fan-in | 语义判断 |
| --- | ---: | --- |
| `agentdash-domain` | 21 | 合理稳定底座候选 |
| `agentdash-platform-spi` | 19 | 高风险：名义 SPI 实际 re-export concrete Agent engine |
| `agentdash-diagnostics` | 18 | dependency-light 横切关注点，基本合理 |
| `agentdash-agent-runtime-contract` | 15 | 共享执行合同；属于 Agent Runtime 专项审计范围 |
| `agentdash-application-ports` | 14 | 稳定端口候选，但间接受 platform-spi concrete leakage 污染 |
| `agentdash-agent-protocol` | 13 | canonical protocol + vendor projection，需 generator 守护 |
| `agentdash-agent-service-api` | 13 | Complete Agent seam；属于 Runtime 专项审计范围 |

关键依赖层次：

```text
Interface/composition
  agentdash-api / agentdash-local / agentdash-local-tauri / agentdash-mcp
        |
        +--> application aggregate + application vertical slices
        +--> infrastructure implementations
        +--> integration adapters / relay / runtime-wire

Application
  agentdash-application
  agentdash-application-{ports,agentrun,lifecycle,workflow,vfs,hooks,...}
        |
        +--> domain / platform-spi / contracts

Nominal stable foundations
  agentdash-domain
  agentdash-application-ports
  agentdash-platform-spi   -- currently depends on concrete agentdash-agent
  agentdash-{agent-protocol,agent-runtime-contract,agent-service-api}

Adapters
  agentdash-infrastructure -- currently also owns product/runtime composition
  agentdash-integration-{native-agent,codex,remote-runtime}
```

#### 2.2 pnpm package DAG

当前正式 package graph：

```text
@agentdash/core      @agentdash/ui
       \               /
        +--> @agentdash/views
              \       |
               +--> app-web
                      |
                      +--> app-tauri

@agentdash/extension  (独立 toolchain/browser/host package)
```

证据：

- `@agentdash/core` 无 workspace dependency，导出通用 local-runtime contract
  （`packages/core/package.json:9-12`）；
- `@agentdash/ui` 无 workspace dependency（`packages/ui/package.json:13-29`）；
- `@agentdash/views` 只依赖 core/ui（`packages/views/package.json:15-18`）；
- `app-web` 依赖 core/ui/views（`packages/app-web/package.json:20-24`）；
- `app-tauri` 作为桌面 composition 依赖 app-web 与三 shared packages
  （`packages/app-tauri/package.json:14-24`）；
- `app-tauri/src/App.tsx:2` 直接将 `WebDashboardApp` 作为被托管应用，并在
  `app-tauri/src/App.tsx:30-36` 安装 desktop bridges。

**结论：前端 package 层无循环，依赖方向清晰。** `app-tauri -> app-web` 是产品壳复用，不应因
“依赖 app package”机械判错。真正风险在 `app-web` 内部 feature/store 的语义 owner，另有前端专项
审计负责。

---

### 3. 风险结论（按优先级）

#### R1 — Critical：`platform-spi` 是名义稳定端口、实际 concrete Agent API 聚合器

**证据**

- `agentdash-platform-spi` production 依赖 `agentdash-agent`
  （`crates/agentdash-platform-spi/Cargo.toml:8-12`）；
- `platform-spi/src/lib.rs:8-30` 明写“保持外部 API 不变”，从 `agentdash-agent` re-export
  Agent message、compaction、hook delegate、tool、protocol projector 等大批 concrete 类型；
- `platform/runtime_surface.rs:7-13` 直接 import `AgentMessage`、
  `AgentRuntimeDelegateSet`、`MessageRef`；
- `platform/runtime_surface.rs:201-219` 的 `ExecutionTurnFrame` 直接持有 concrete
  `AgentRuntimeDelegateSet` 与 `Vec<agentdash_agent::DynAgentTool>`；
- 19 个 production crate 直接依赖 platform-spi，14 个依赖 application-ports，而
  application-ports 又依赖 platform-spi（`crates/agentdash-application-ports/Cargo.toml:7-12`）。

**为什么不是“合理的共享类型”**

SPI 应由调用方/平台拥有稳定需求合同，implementation 应依赖 SPI。当前反过来由 SPI 依赖并
re-export concrete Agent engine；它把 engine 的 message/tool/hook/compaction shape 传播给
application、infrastructure、contracts、API、Local、MCP。production graph 仍是 DAG，但
Stable Dependencies Principle 已失效。

**典型触发器与爆炸半径**

- 修改 Agent tool result、hook delegate、message 或 compaction 类型；
- 直接触发 platform-spi 公共 API 变化；
- 继而影响 application-ports 的所有 consumer；
- 最远传播到 infrastructure、API、Local、MCP、contracts/generated TS。

**历史印证（Agent Runtime 之外）**

Pi Agent responses tool draft 事故先后出现：

- `6c41baf`：内部 tool draft 透传；
- `fc8d9bb`：wire API 配置链路；
- `0b24e3b`：最终升级 `rig-core` 并同步修改 Agent loop、provider registry、bridge。

本地会话 `019d4925-478a-79f3-957c-e4e8b1f73167` 记录：上游 provider 的
tool delta 缺 name/start，变化必须穿过 dependency、provider bridge、agent loop 与 connector。
历史路径来自旧 crate 布局，当前文件可能已重构；commit 锚点用于证明演化传播，不声称旧路径仍存在。

**风险等级**：Critical  
**根因类别**：依赖倒置失败 / concrete type leakage / compatibility re-export  
**建议目标边界**

1. hard cut 建立 dependency-light `agentdash-agent-api`（或等价命名），只放 platform 所需的
   Agent message/tool/hook delegate contracts；
2. `agentdash-agent` 实现该合同，`platform-spi` 只依赖合同 crate；
3. 删除 `platform-spi` 的 concrete re-export，不保留兼容 facade；
4. 让 `ExecutionTurnFrame` 持有 platform-owned trait object/value contract，而不是 engine
   concrete types。

**自动守护**

- manifest architecture test：禁止 `platform-spi -> agentdash-agent`；
- `cargo metadata` 规则：foundation allowlist 只能依赖 domain/contract/serde/async primitives；
- API surface snapshot：platform-spi public type 不得来自 concrete engine crate。

---

#### R2 — High：`agentdash-infrastructure` 混合 persistence adapter 与 production composition

**证据**

- manifest 有 20 个 production internal dependencies，包含：
  - application-agentrun/hooks/vfs/workflow；
  - runtime/host/service；
  - integration-api/native-agent；
  - llm-provider、workspace-module；
  见 `crates/agentdash-infrastructure/Cargo.toml:7-26`；
- `infrastructure/src/lib.rs:1-20` 同时暴露 persistence、migration、HTTP skill source、
  script runtime、MCP probe、runtime tool executors、Complete Agent composition；
- `infrastructure/src/lib.rs:22-35` 导出 Product/Complete Agent composition；
- `infrastructure/src/lib.rs:36-72` 导出约 36 个 PostgreSQL repository；
- `infrastructure/src/lib.rs:73-88` 又导出 production service selection、shell terminal、
  product tool authorization 与 runtime tool catalog；
- `agent_run_product_projection_repository.rs:2` 直接依赖 application-agentrun 类型；
- `complete_agent_product_provisioning.rs:30-42` 同时依赖 application product frame 与
  application ports；
- `runtime_tool_executors.rs:13-21` 同时依赖 application-agentrun、ports 与 VFS。

**语义判断**

Infrastructure 实现 domain/application port 本身合理；不合理的是它还拥有 Product projection
composition、Complete Agent provisioning、service selection、runtime tool policy。持久化 adapter
因而无法独立于应用和 integration 演进。

这直接偏离 backend invariant：“Infrastructure 实现 Domain/SPI 持久化端口，不依赖 application
编排层”（`.trellis/spec/backend/architecture.md`）。

**典型触发器与爆炸半径**

- application-agentrun frame/binding/terminal 类型变化；
- integration-native-agent service selection 变化；
- runtime tool catalog/VFS policy 变化；
- 会同时重编 infrastructure 的 repository、composition 与 API，数据库测试与执行测试也被绑在一起。

**风险等级**：High  
**根因类别**：adapter/composition ownership conflation  
**建议目标边界**

1. `agentdash-persistence-postgres`：只依赖 domain、application-ports、必要 canonical contracts；
2. `agentdash-platform-adapters`：Rhai、HTTP、filesystem、RMCP 等无业务装配的具体 adapter；
3. `agentdash-production-composition`：唯一允许同时依赖 application、persistence、integration、
   runtime host 的 composition crate；
4. API/Local composition root 只消费 subsystem builder/result，不直接穿透 repository concrete。

**自动守护**

- 禁止 persistence crate 依赖 `agentdash-application-*` 的具体 use-case crate与
  `agentdash-integration-*`；
- repository module 的 import allowlist；
- production composition test：删除任一 required adapter 时必须编译或测试失败，防止隐式 optional
  装配。

---

#### R3 — High：`agentdash-application` 是“拆分后仍保留全部旧实现”的变化汇聚层

**证据**

- production fan-out 20；
- `Cargo.toml:7-26` 同时依赖 agent、protocol/contracts、所有主要 application vertical slices、
  domain/SPI/relay/workspace-module；
- source 仍约 44,955 行；
- `src/lib.rs:1-3,18-20,34-45` 一方面 re-export 已拆出的 agentrun/lifecycle/skill/vfs，
  另一方面 `src/lib.rs:4-48` 继续拥有 auth/backend/capability/companion/context/interaction/
  project/routine/task/workspace 等大量真实实现；
- production consumer 只有 API、Local、MCP 三个入口，但三者因此共享整个 aggregate
  compilation boundary：
  - API 多处直接 import；
  - Local manifest 直接依赖 aggregate（`crates/agentdash-local/Cargo.toml:24-26`）；
  - MCP manifest 直接依赖 aggregate（`crates/agentdash-mcp/Cargo.toml:8`）。

**语义判断**

这是“迁移 facade + 仍承载大量真实业务”的双重角色，不是稳定 facade。任一未拆模块变化都会让
Local/MCP/API 同步重编；任一已拆 slice 又可能经 aggregate re-export 形成两种合法 import 路径。

**风险等级**：High  
**爆炸半径**：所有 Cloud/Local/MCP entrypoint；跨 domain 的 test/support；API error mapping  
**建议**

- 按现有拆分方向完成 hard cut；
- API/MCP/Local 直接依赖 intent-specific application crate；
- 删除 aggregate re-export 与空壳，不维持旧 import path；
- 每个 vertical slice 明确 owned domain、ports、DTO 禁止反向跨 slice import。

**守护**

- 禁止新增 `agentdash-application` production consumer；
- 禁止 aggregate re-export sibling application crate；
- 新功能必须落在 intent-specific crate，架构测试检查 import boundary。

---

#### R4 — Medium-High：API 高 fan-out 大体合理，但 composition concrete 泄漏到 route

**合理部分**

`agentdash-api` 是最终 Cloud composition root，依赖 29 个 internal crate 不构成独立坏味道。
`bootstrap/repositories.rs:82-123` 集中实例化 PostgreSQL repositories；
`bootstrap/vfs.rs:35-78` 集中装配 mount providers/VFS；这些是合理 composition。

**失败部分**

- `AppState::new_with_integrations` 从 `app_state.rs:247` 延伸到约 `:820`，单个构造函数同时装配
  runtime、VFS、hook、projection、tools、workflow、extension、routine；
- `ServiceSet` 有 48 个左右字段（`app_state.rs:170-220`）；
- 字段直接暴露 concrete infrastructure type：
  - `PostgresAgentRunProductRuntimeBindingRepository`（`:179`）；
  - `PostgresAgentRunTerminalProjectionStore`（`:193`）；
  - `PostgresWorkflowRecoveryRepository`（`:219`）；
- route/handler 直接消费这些 concrete：
  - runtime binding：`agent_run_runtime_surface.rs:65`、
    `routes/companion_gates.rs:50`、`routes/vfs_surfaces/resolver.rs:42`；
  - terminal store：`routes/terminals.rs:361`；
  - product projection composition：`routes/lifecycle_agents.rs:344,748`。

**语义判断**

高 fan-out 是合理 composition，`AppState` 将 concrete repository 暴露给 interface handler 则使
composition 约束扩散。route 不再只依赖 application query/command port，repository shape 变化会穿透
HTTP 层。

**风险等级**：Medium-High  
**建议**

- 以 subsystem state 分组：Identity、Project、Workflow、AgentProduct、Integration、VFS；
- 每组 builder 返回 application-owned command/query ports；
- route state 只公开意图级 facade，不公开 Postgres concrete；
- `AppState` 最终只组合 subsystem handle。

**守护**

- API route import denylist：不得引用 `agentdash_infrastructure::Postgres*`；
- handler constructor test 只接受 trait/facade；
- subsystem composition smoke tests 替代一个全知 `AppState` fixture。

---

#### R5 — Medium：生成契约是正确的稳定化手段，但生成 DAG 依靠命令顺序和跨输出读取

**当前 pipeline**

根命令按固定顺序串行 6 个生成器（`package.json:57-58`）：

1. upstream Codex protocol；
2. private Codex vendor audit；
3. frontend/backend DTO contracts；
4. Agent Runtime contract；
5. Complete Agent Service API；
6. Runtime Wire。

输出/依赖证据：

- `agentdash-contracts` 写 `backbone-protocol.ts`
  （`generate_ts.rs:330-335`）；
- Runtime contract generator读取 `backbone-protocol.ts`
  （`agentdash-agent-runtime-contract/src/generate.rs:13-28`）；
- Service API generator也读取 `backbone-protocol.ts`
  （`agentdash-agent-service-api/src/generate.rs:14-39`）；
- Wire generator读取前两者生成的 TS
  （`agentdash-agent-runtime-wire/src/generate.rs:14-44`）；
- Wire generator对 owned type collision/import closure 有显式检查（`:45-73`）；
- 所有 generator 提供 check mode，`contracts:check` 已进入 full local gate
  （`scripts/lib/quality-gates.js:151-166`）。

**合理部分**

- 单向 Rust authority -> generated TS；
- generator 有 drift check；
- Runtime/Service/Wire 对类型 owner collision 做负向校验；
- 236 个 Codex vendor TS 有独立 upstream generator ownership。

**风险**

- 生成 DAG 只编码在 shell command 顺序；
- 后置 generator读取前置 generator 的 checked-in output，而非共享内存 schema artifact；
- `agentdash-contracts` 仍依赖 application-ports 与 platform-spi
  （`crates/agentdash-contracts/Cargo.toml:14-20`），内部 port/SPI 变化可进入 frontend codegen closure；
- DTO generator 单文件约 1,322 行，新增 domain 容易继续汇聚。

**风险等级**：Medium  
**爆炸半径**：contracts crate、schemas、约 273 generated TS、frontend services/types/tests  
**建议**

- 建立机器可读 generator manifest：owner、inputs、outputs、depends_on、digest；
- 守护“一输出唯一 generator”和 DAG 无环；
- contracts 显式定义 wire DTO，避免直接复用 application port/SPI concrete；
- quality gate 按 manifest 拓扑执行，不依赖手写 `&&` 顺序。

---

#### R6 — Medium：production 无环，但测试装配存在三 crate SCC

production+test graph 的唯一多节点 SCC：

```text
agentdash-application-agentrun
    --production--> agentdash-application-workflow
    --dev---------> agentdash-infrastructure
    --production--> agentdash-application-agentrun
```

证据：

- AgentRun production 依赖 Workflow
  （`crates/agentdash-application-agentrun/Cargo.toml:7-16`）；
- Workflow 只在 dev-dependencies 依赖 Infrastructure
  （`crates/agentdash-application-workflow/Cargo.toml:26-29`）；
- Infrastructure production 依赖 AgentRun 与 Workflow
  （`crates/agentdash-infrastructure/Cargo.toml:7-14`）。

**必须明确：这不是 production runtime cycle。** Cargo production DAG 无 SCC；该环意味着 Workflow
测试要借 Infrastructure concrete，而 Infrastructure 又编译 Product/Workflow concrete。它增加
测试构建变化传播与 test fixture ownership 模糊。

`agentdash-test-support` 已集中 stateful fake repositories，fan-in 10；例如
`test-support/src/workflow.rs:18-181` 提供 LifecycleRun/AgentFrame fake，
`:243-487` 又扩展 Workflow/Lineage fake。集中 fake 是合理复用，但单文件约 843 行，已经成为 domain
test schema 的同步热点。

现有 guard `scripts/check-test-support-boundaries.js:8-16,41-79` 只按
`Memory|InMemory|Fake|Mock|Test` + `Repository|Repo|Store` 命名识别，显式
`RecordingX`、`FixtureX` 或匿名 adapter 可绕过；它保护位置，不验证 fake 与 port 的行为一致。

**风险等级**：Medium  
**建议**

- Workflow 单测使用 application-port fake；需要真实 PostgreSQL 的用例迁到 infrastructure/composition
  integration test；
- 按 domain 拆 test-support modules，避免 `workflow.rs` 吸收所有 repository shape；
- 为 repository contract 建可复用 conformance suite，让 memory/Postgres adapter跑同一行为集合；
- guard 从命名正则升级为依赖/trait impl AST 或 rustdoc metadata 检查。

---

#### R7 — Medium：migration 集中是正确事实边界，但 schema/readiness 仍是同步修改热点

**当前事实**

- migration 只有 `crates/agentdash-infrastructure/migrations/0001_init.sql`；
- 当前文件匹配到 57 个 `CREATE TABLE`；
- SQLx 入口唯一：`migration.rs:177-183`；
- readiness 在 Rust 再列一次 required/retired tables：
  - required：`migration.rs:4-61`；
  - retired：`migration.rs:63-175`；
  - 检查入口：`:186-190`；
- API build script再次枚举 migration 文件计算 schema version
  （`crates/agentdash-api/build.rs:4-28`）；
- migration history guard 对已提交 migration 禁止普通 rewrite
  （`scripts/check-migration-history.js:71-113`），并已进入 quick/full gate
  （`scripts/lib/quality-gates.js:102-124,151-166`）。

**历史 churn**

归档任务 `07-24-database-schema-convergence` 记录：

- 当时有 52 张业务表、607 字段、116 段 migration
  （`prd.md:9-12`）；
- 发现无读者表、write-only outbox、重复 projection、同 owner 状态拆表
  （`prd.md:15-20`）；
- 最终将 `0002`～`0116` 压成单一 baseline
  （`design.md:72-79`、`implement.md:32-39`）。

该历史证明 persistence 曾经按“实现动作/未来需求”增长，而不是按独立事实 owner 增长；这是稳定
边界失败的真实样本。当前单一 baseline 与 guard 是明显改善。

**不要误判**

单个 migration 文件被多个 domain 同步修改是数据库 schema 事实源的必要 composition，不应仅凭
co-change 拆成每 domain 一套 database。风险在：

- table owner/read consumer 不清；
- SQL 与 Rust readiness 双录；
- 新表是否有独立事实/读取链缺少自动门禁。

**风险等级**：Medium（历史为 High，当前守护后下降）  
**建议**

- baseline 内按 domain owner 加机器可读 ledger（table -> owner crate -> repository -> read consumer）；
- readiness 从 SQLx migration metadata/声明式 ledger生成，避免手写双录；
- 加 orphan/write-only schema 审计：有 insert 无 authoritative read 的表失败；
- PostgreSQL contract suite覆盖空库、已存在开发库 migration、retired table absent；
- baseline reset 仍只在明确授权任务执行，不把 pre-release hard cut 变成日常习惯。

---

### 4. 历史事故与 co-change 语义分类（Agent Runtime 之外）

| 历史样本 | 锚点 | 同步变化面 | 分类 | 结论 |
| --- | --- | --- | --- | --- |
| API structured error 过大触发全仓 `result_large_err` | commit `5266d5e7`; mem session `019ec49e-c2b0-71a0-90cd-c46a813f786d` | `rpc.rs` + 两个 route 构造/匹配 | **合理内聚** | 共享 error representation 影响所有 `Result<_, ApiError>` 是 Rust 类型系统的合理传播；最终只改 API 内 3 文件，clippy + serialization test 已守护，不应据此拆 API |
| Pi Agent responses tool draft 晚出现 | commits `6c41baf`, `fc8d9bb`, `0b24e3b`; mem session `019d4925-478a-79f3-957c-e4e8b1f73167` | dependency/provider bridge/Agent loop/connector | **边界失败** | provider delta 语义未被 anti-corruption contract完整吸收，导致升级穿透 core；与 R1 concrete Agent API leakage 同类 |
| Extension loadability 被后端与前端重复推断 | mem session `019f14a3-09d0-7182-af65-d03b3894e6cc` | workspace-module + frontend bridge/test + generated TS | **边界失败** | authoritative loadability 已在 backend，frontend 仍按 artifact/entry二次判断；修复要求删前端推断并同步 generator，证明 projection owner 曾不唯一 |
| 数据库 116 migrations / 无读者表与 write-only outbox | `.trellis/tasks/archive/2026-07/07-24-database-schema-convergence/{prd,design,implement}.md` | domain/repository/composition/schema/readiness/tests | **边界失败，已大幅收敛** | 表按实现过程而非独立事实 owner 生长；baseline 重建与 owner-state 回收是正确方向 |
| 兼容/fallback 债务全栈清理 | `.trellis/tasks/archive/2026-04/04-01-cleanup-compat-fallback-debt/progress.md:58-92` | frontend mapper、API、application、repository、relay、local/dev scripts | **系统性边界失败** | 缺字段/坏枚举/owner/workspace 被各层自行猜测，造成多个“看起来可用”的事实源；不是一次功能 bug，而是严格 contract 缺失导致的长期 co-change |

#### 4.1 API error 样本为何是合理 composition

会话记录显示 `ApiError::ConflictWithCode` 大载荷使大量返回 `ApiError` 的函数同时触发 clippy；
将 payload Box 化后，只需：

- `rpc.rs` 修改 owner type；
- `lifecycle_agents.rs`、`file_picker.rs` 更新两个 construction/match consumer；
- 运行 workspace clippy 与结构化响应测试。

这种共变发生在同一 HTTP error boundary，爆炸半径由编译器精确暴露，且修复未穿透 application/domain。
它是“高 fan-in 共享类型的合理代价”，不能把所有 co-change 热点都判成边界失败。

#### 4.2 Extension 样本为何是稳定边界失败

会话 `019f14a3-09d...` 记录的修复：

- frontend `webviewBridge` 删除对 `package_artifact` / renderer entry 的重复可用性推断；
- `agentdash-workspace-module` 修正 authoritative ready 判定；
- 重新生成 `extension-runtime-contracts.ts`。

同一“是否可加载”概念由 backend 与 frontend共同解释，才导致跨语言同步修复。正确边界是 backend
owner 发布 typed `loadability`，frontend 只渲染。这个样本应转化为通用 projection owner guard。

#### 4.3 fallback debt 为何比单个 bug 更重要

归档记录在一次清理中同时删除：

- “第一个 workspace / executor / preset”猜测；
- 非法 enum/JSON -> 空串/默认状态/当前时间；
- NDJSON -> SSE 隐式降级；
- bad owner/session binding -> 无上下文；
- 非 PostgreSQL URL -> embedded fallback；
- relay MCP/JSON 解析失败 -> warn/skip。

证据位于 `progress.md:58-92` 与 `subtasks.md:77-123`。这些不是模块数量问题，而是没有单一
authoritative contract 时，每层都补了一份 policy。未来边界守护应优先防“consumer 自行推断”，
而不是只限制 import。

---

### 5. 合理 hub 与失败 hub 总结

| hub | 当前判断 | 理由 |
| --- | --- | --- |
| `agentdash-domain`（fan-in 21） | 合理稳定底座 | domain-owned entity/value/repository contracts；无上层 internal dependency |
| `agentdash-diagnostics`（fan-in 18） | 合理横切底座 | dependency-light diagnostics；变化通常是观测合同 |
| `agentdash-api`（fan-out 29） | composition 合理、concrete 泄漏不合理 | 最终入口必须装配全系统；route 不应看到 Postgres concrete |
| `agentdash-platform-spi`（fan-in 19） | 稳定边界失败 | 直接依赖/re-export concrete `agentdash-agent` |
| `agentdash-infrastructure`（fan-out 20） | 稳定边界失败 | persistence、integration selection、runtime/product composition 混合 |
| `agentdash-application`（fan-out 20） | 迁移未完成的变化 hub | aggregate re-export 与 45k 行真实业务并存 |
| `packages/app-web/src/generated` | 合理生成 hub、有顺序风险 | canonical Rust -> TS 是稳定化；生成 DAG 依赖 shell 顺序 |
| 单一 `0001_init.sql` | 合理 schema composition | 数据库 schema 必须有单一事实源；owner/read chain 才是拆分判据 |
| `app-tauri -> app-web` | 合理产品壳 composition | desktop shell 托管同一 Web dashboard，而非业务反向依赖 |

---

### 6. 建议整改顺序与预期爆炸半径变化

#### P0：恢复 foundation 依赖方向

1. 从 `platform-spi` 移除 concrete `agentdash-agent` dependency/re-export；
2. 抽 dependency-light Agent API/tool/hook contract；
3. architecture test 禁止 foundation -> implementation。

**预期变化**：Agent/provider/tool/hook 内部变化不再穿透 19 个 platform-spi consumer。

#### P1：分离 persistence 与 production composition

1. PostgreSQL repositories进入纯 persistence adapter；
2. Product/Complete Agent provisioning、tool catalog、selection进入 composition crate；
3. API route只消费 application facade。

**预期变化**：repository/schema 变化只影响 ports + persistence + composition tests；
Agent product/integration 变化不再重编全部 persistence。

#### P2：完成 application aggregate hard cut

1. 继续把剩余业务拆为 intent-specific application crates；
2. API/Local/MCP直接依赖所需 slice；
3. 删除 aggregate facade/re-export。

**预期变化**：例如 MCP preset、workspace detection、task plan 的变化不再让三个 entrypoint共享
整个 45k 行 application compilation boundary。

#### P3：收窄 composition state

1. `AppState` 按 subsystem builder/handle 分组；
2. concrete repository藏在 composition内部；
3. route state只公开 query/command facade。

**预期变化**：新增/替换 repository 不要求修改 route；AppState merge/churn 热点分散到 owner builder。

#### P4：机器化 contract/migration/test ownership

1. generator DAG manifest + unique output owner；
2. schema table owner/read-consumer ledger；
3. repository memory/Postgres conformance suite；
4. 解除 Workflow dev -> Infrastructure 测试环。

**预期变化**：同步修改仍存在，但变成由生成/合同测试证明的必要共变，而不是人工记忆的隐式共变。

---

### 7. 可直接拆分的后续 Trellis 子任务

1. **Platform SPI concrete Agent hard cut**
   - 验收：`platform-spi` manifest 不含 `agentdash-agent`；public API 无 concrete re-export；
     19 个 consumer 编译/测试通过。
2. **Persistence/composition crate split**
   - 验收：Postgres crate不依赖 application use-case/integration crate；production composition有独立
     smoke test。
3. **Application aggregate retirement**
   - 验收：API/Local/MCP不依赖 `agentdash-application` aggregate；旧 crate从 workspace删除。
4. **API subsystem composition handles**
   - 验收：route源码无 `Postgres*` concrete；AppState按 subsystem分组；现有 route tests通过。
5. **Generated contract DAG manifest**
   - 验收：所有 generated output有唯一 owner，拓扑可机器验证，删错/重复 output 会失败。
6. **Schema owner/read-chain gate**
   - 验收：57 个当前 table 均有 owner/repository/authoritative reader声明；write-only/orphan table gate
     可制造失败。
7. **Workflow test cycle removal**
   - 验收：production+dev internal graph无 SCC；真实 PostgreSQL workflow test归 infrastructure/
     composition owner。
8. **Cross-layer projection no-reinterpretation guard**
   - 验收：Extension loadability 等 typed projection在 frontend只消费不推断；新增同类 projection有
     contract fixture/sentinel test。

## Files Found

| 文件 | 一句话说明 |
| --- | --- |
| `Cargo.toml:1-45,103-145` | 40-crate workspace 与 internal/upstream dependency registry |
| `crates/agentdash-platform-spi/Cargo.toml:8-29` | SPI 直接依赖 concrete Agent/Runtime/Domain |
| `crates/agentdash-platform-spi/src/lib.rs:8-30` | concrete Agent types 的兼容 re-export 面 |
| `crates/agentdash-platform-spi/src/platform/runtime_surface.rs:201-219` | platform frame 持有 Agent engine concrete delegate/tool |
| `crates/agentdash-infrastructure/Cargo.toml:7-26` | persistence crate 的 20 个 production internal deps |
| `crates/agentdash-infrastructure/src/lib.rs:1-93` | persistence/composition/tools/selection 混合出口 |
| `crates/agentdash-application/Cargo.toml:7-26` | aggregate application 的 20 个 production internal deps |
| `crates/agentdash-application/src/lib.rs:1-51` | re-export vertical slices 与旧真实模块并存 |
| `crates/agentdash-api/Cargo.toml:15-43` | Cloud composition root 广泛依赖 |
| `crates/agentdash-api/src/app_state.rs:170-240,247-820` | 超宽 ServiceSet 与单体 composition constructor |
| `crates/agentdash-api/src/bootstrap/repositories.rs:82-123` | 合理的 repository composition |
| `packages/app-web/package.json:20-24` | Web app 对 shared packages 的单向依赖 |
| `packages/app-tauri/package.json:14-24` | Desktop shell 对 app-web/shared package 的合理 composition |
| `packages/app-tauri/src/App.tsx:2,30-36,140` | 托管 Web app 并注入 desktop bridge |
| `package.json:51-63` | migration/test/contract/quality gates 与 generator 顺序 |
| `crates/agentdash-contracts/src/generate_ts.rs:291-335` | DTO/backbone generator 入口 |
| `crates/agentdash-agent-runtime-wire/src/generate.rs:31-73` | 后置 generator 对前置 TS output 的读取与 collision 检查 |
| `crates/agentdash-infrastructure/migrations/0001_init.sql:17-1150` | 当前单一 PostgreSQL baseline，57 个 table |
| `crates/agentdash-infrastructure/src/migration.rs:4-190` | required/retired schema readiness 双录 |
| `scripts/check-migration-history.js:71-113` | migration immutable history guard |
| `scripts/check-test-support-boundaries.js:8-79` | stateful fake 位置的命名正则 guard |
| `crates/agentdash-test-support/src/workflow.rs:18-843` | 集中的 workflow/lifecycle memory repositories |
| `.trellis/tasks/archive/2026-07/07-24-database-schema-convergence/prd.md:5-31` | 116 migrations/52 tables 与 orphan/write-only schema 历史 |
| `.trellis/tasks/archive/2026-04/04-01-cleanup-compat-fallback-debt/progress.md:58-92` | 全栈 fallback/重复 policy 历史证据 |

## Code Patterns

### Pattern A：编译 DAG 正确但稳定方向错误

`platform-spi -> agentdash-agent` 和 `infrastructure -> application/integration` 都不会形成 production
SCC，却会让高层实现变化流向名义底座。架构守护必须检查 layer allowlist，不能只检查 cycle。

### Pattern B：composition root 的依赖广度是必要的，concrete 暴露不是

API bootstrap实例化所有 adapter 是合理的；将 Postgres repository放入 public `ServiceSet` 并交给
route使用，会把 wiring detail升级成 interface contract。

### Pattern C：生成物能隔离语言边界，但 generator 本身也需要 owner/DAG

当前 drift check 和 collision check是好模式；手写 command 顺序、后置 generator读取前置 checked-in
TS 是隐式 composition constraint。

### Pattern D：单一 schema 文件是事实收敛，不等于低 churn

数据库必须集中 composition；应以“独立事实 owner + 真实 read/claim contract”判断 table，而不是按
业务目录拆 migration。

### Pattern E：测试 fake 集中可复用，但不能替代 adapter conformance

正则可阻止明显的 scattered fake，却不能证明 fake 与 Postgres实现相同行为；共享 fake随 port变化
同步修改是合理的，行为漂移才是风险。

## External References

本次未访问互联网，未使用外部架构文章作为结论依据。版本/上游锚点来自仓库 manifest：

- Rust edition 2024（`Cargo.toml:46-49`）；
- pnpm 11.9.0（`package.json:7`）；
- Codex Rust crates固定 `rust-v0.144.1`
  （`Cargo.toml:142-145`）；
- ACP SDK前端 `^0.14.1`（`packages/app-web/package.json:21`）。

历史 Pi Agent 样本中的 `rig-core` 版本信息来自本地会话记录，只作为事故时间线，不作为当前
manifest事实。

## Related Specs

- `.trellis/spec/project-overview.md`：接口稳定、实现可变与 Cloud/Local owner。
- `.trellis/spec/backend/architecture.md`：Interface -> Application -> Domain/SPI，
  Infrastructure 实现 port 且不依赖 application orchestration。
- `.trellis/spec/backend/directory-structure.md`：crate 分层基线。
- `.trellis/spec/backend/database-guidelines.md`：PostgreSQL/migration authority。
- `.trellis/spec/backend/shared-library.md`：共享库业务边界。
- `.trellis/spec/frontend/architecture.md`：frontend package/feature role。
- `.trellis/spec/frontend/directory-structure.md`：app-web feature/store/service 组织。
- `.trellis/spec/cross-layer/architecture.md`：跨层 owner 与 failure patterns。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：Rust DTO -> generated TS。
- `.trellis/spec/cross-layer/shared-library-contract.md`：共享资产跨层合同。
- `.trellis/spec/guides/cross-layer-thinking-guide.md`：纵向数据流/事实源检查。

## Caveats / Not Found

1. **没有直接 Git 定量数据。** 研究角色明确禁止任何 Git 命令；未计算 commit-level co-change pair、
   churn、Jaccard/lift 或作者/时间窗。主审计流会补齐。本文只提供 task/session/commit 锚点的定性样本。
2. **production SCC 与 test SCC 已严格分开。** 唯一三节点 SCC 含 Workflow 的 dev-dependency，
   不能表述成运行时循环。
3. **manifest parser 口径。** production 包含普通/build dependency；dev 单列。未对 feature
   activation frequency 加权，optional edge也按声明存在计入。
4. **并行工作区。** 数据来自 2026-07-26 当前文件系统；其他会话可能正在修改 workspace。
   当前 57-table/文件行数是观察值，不保证等于某个 clean commit。
5. **未执行 compile/test。** 本任务是研究审计，未修改产品代码，也未跑会写 build cache或数据库的
   全量测试；证据为 manifest/source/static scan 与既有验证记录。
6. **前端内部 feature import SCC 未在本报告重算。** package DAG 已核验，feature/store owner由并行
   frontend审计覆盖，避免重复结论。
7. **Agent Runtime 风险只作图节点。** 按用户要求，本报告的历史事故样本优先选择 API、Pi provider、
   Extension、Database 与 fallback debt；Agent Runtime/AgentRun 的详细 owner事故应由其它专题报告给出。
