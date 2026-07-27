# Research: 架构与边界约束防复发可实施性

- Query: 基于现有质量门、workspace/package manifest、生成器、migration guard、production
  registration/composition 与 source guard，设计可直接实施的 architecture enforcement system；逐条明确
  权威输入、检查形态、现有接入点、CI gate、valid/invalid/missing-root self-test、失败消息、例外模型、
  启用顺序和后续 work package。
- Scope: internal
- Date: 2026-07-26

## Findings

## 1. 结论

这套系统可以直接实施，但不能继续以“新增若干 grep 脚本”的方式扩张。当前仓库已经有一个适合作为统一
入口的质量门 manifest，也有生成器 check mode、migration guard、若干 registration table 和局部
composition tests；缺失的是一层能把这些检查绑定到真实 workspace、owner ledger 和 production
composition 的 architecture harness。

建议的最终结构：

```text
Cargo metadata / pnpm workspace packages / migration DDL / code-owned registrations
                                  +
architecture manifests（只声明 layer、owner、允许边、capability/data/contract 分类）
                                  |
                                  v
repository-owned architecture harness
  ├─ workspace + active-doc projection
  ├─ dependency + public API + source ownership
  ├─ entrypoint classification + reachability
  ├─ generated DAG + runtime decoder
  ├─ data owner + migration range guard
  └─ frontend owner + desktop/relay/path parity
                                  |
              +-------------------+-------------------+
              v                   v                   v
        static PR gate      behavior/integration    runtime metrics
        必须全绿           必须真实失败注入         只观察、不代替 gate
```

实现时必须坚持四个约束：

1. **扫描根从真实 workspace 推导。** Rust root 来自 `cargo metadata.workspace_members`，pnpm root
   来自 `pnpm -r list --depth -1 --json`；guard 不再维护 `scanRoots` 数组。
2. **machine manifest 不复制业务 schema。** 它只拥有 package layer、public owner、capability/data/
   contract 分类和允许边；DTO、DDL、route table、tool descriptor 和状态机仍由各自代码 owner 持有。
3. **声明与实现双向核对。** 新 workspace member、table、generated output、route/tool/worker/Tauri
   command 或 frontend feature 未分类时失败；manifest 指向已删除 root/symbol/output 时也失败。
4. **每个 guard 先证明自己会失败。** valid、invalid、missing-root 三类 fixture 是 architecture gate
   的组成部分，不是开发时一次性测试。

现有违规不应形成 warning、永久 baseline 或“只阻止新增”式完成态。某类规则可以先以 fixture 完成
checker self-test，但必须在消除当前违规的 work package 末尾以 blocking gate 启用；未启用不能计作
防复发完成。

## 2. 当前可复用基础与直接缺口

### 2.1 质量门入口可复用，但 gate coverage 尚未闭合

`scripts/lib/quality-gates.js:1-9,107-166` 已把 gate/step 组成集中到一个 manifest；
`scripts/quality-gates.js:35-125` 负责展开和执行；四个 GitHub workflow 均调用这个 runner，而不是各自
复制命令。这个结构应保留，并增加 architecture steps/gates。

当前缺口：

- `pr_quick` 只有 migration、test-support、shared/frontend/backend check
  (`scripts/lib/quality-gates.js:116-124`)；
- `contracts_check` 只进入 `full_local` (`:151-166`)；
- `cloud_image_preflight` 只复用 `pr_quick` (`:147-149`)，所以同样不检查 generated drift；
- `desktop_check` 只有 icon、shared/app-tauri typecheck、Tauri Rust check (`:107-114`)；
- `heavy_check` 是手工触发 workflow，不能承担唯一的 blocking transaction/composition contract；
- `quality:gates:test` 已存在于 `package.json:55`，但没有进入任何 gate，因此 workflow 委托关系和 gate
  成员测试本身不受 CI 保证；
- `pr-quick.yml:4-11` 忽略所有 Markdown，active spec/README 架构投影变化不会触发它；
- 仓库没有 desktop CI workflow；`desktop_check` 目前主要由 `full_local` 间接使用。

因此最小接入不是新增平行脚本，而是在现有 manifest 增加：

```text
architecture_manifest_check
architecture_self_test
architecture_static
architecture_contract
architecture_integration
desktop_contract
```

并让 `pr_quick` 至少包含前三项和 `contracts_check`，`cloud_image_preflight` 继续复用强化后的
`pr_quick`；transaction/composition 的定向行为集进入新的 blocking `architecture_integration`；
desktop parity 进入 PR static gate，packaged invoke/path matrix 进入独立 Windows desktop workflow。

### 2.2 当前 source guard 已证明 missing-root 风险

`scripts/check-background-process-spawn.js:11-17` 手工维护扫描根，其中包含已删除的
`crates/agentdash-executor`。`walk()` 在 root 不存在时直接返回空集合 (`:33-36`)；该脚本也没有
package script、quality gate 或 workflow 调用。它同时展示了三个问题：

- guard root 与 Cargo workspace 重复；
- 删除/重命名 root 会静默缩小覆盖；
- 有脚本不等于存在 gate。

`scripts/check-test-support-boundaries.js:6-20,74-89` 的 root 更好：它扫描整个 `crates/`，不存在时会由
`readdirSync` 失败；但识别规则仍依赖命名正则，不能证明 fake 与 production adapter 行为一致。

现有 active docs/guard 仍引用已删除 crate：

- `.trellis/spec/tech-stack.md:66,69,74-75`
- `.trellis/spec/backend/directory-structure.md:23,26,50,69-70`
- `.trellis/spec/cross-layer/desktop-local-runtime.md:127,183-184`
- `README.md:187`
- `README.zh-CN.md:157`
- `scripts/check-background-process-spawn.js:14`

而当前 Cargo workspace 恰好有 40 个 member、`crates/` 也有 40 个目录。这个样本证明需要的是
`workspace -> manifest -> generated docs/guards` 的闭环，不是人工更新更多路径数组。

### 2.3 generated check mode 已有良好基础，但 DAG 和 decoder 仍不完整

`package.json:57-58` 手写六段 generator 顺序。所有 generator 都支持 check：

- Agent Protocol / Codex Vendor 使用 `write|check`
  (`agentdash-agent-protocol-codegen/src/main.rs:308-376`、
  `agentdash-integration-codex/src/bin/generate_codex_vendor_protocol.rs:312-379`)；
- HTTP contracts、Runtime Contract、Service API、Runtime Wire 使用 `--check`
  (`agentdash-contracts/src/generate_ts.rs:290-295,1249-1270`、
  `agentdash-agent-runtime-contract/src/generate.rs:9-43,157-175`、
  `agentdash-agent-service-api/src/generate.rs:10-49,189-207`、
  `agentdash-agent-runtime-wire/src/generate.rs:10-81,245-262`)。

正向能力：

- missing/outdated output 会失败；
- Agent Protocol generator还能删除/发现 stale output；
- Runtime/Service/Wire 有 required declaration 和 type collision self-test；
- Wire 会显式读取 Runtime/Service generated output，并在前置 output 缺失时报错
  (`agentdash-agent-runtime-wire/src/generate.rs:31-50`)；
- 通用 generator 已生成 Project/Session NDJSON validators
  (`agentdash-contracts/src/generate_ts.rs:1112-1115`；
  `packages/app-web/src/generated/ndjson-stream-validators.ts`)。

剩余缺口：

- DAG 只编码在 `package.json` 的 `&&` 顺序，output owner、depends_on、input/output digest 不可查询；
- 后置 generator 读取 checked-in TS output，manifest 不知道这条依赖；
- `agentdash-contracts` 的 check path 会先 `create_dir_all`
  (`generate_ts.rs:1207,1230,1240`)，check mode 不是严格只读；
- 通用 Project/Session validator 与 Service API codec 分属不同手写模板，生成系统没有声明
  “哪类 wire 必须有哪一个 decoder”；
- `agent-service-codecs.ts` 有 command/snapshot/change/surface 等 decoder，但没有
  `decodeAgentLiveEvent`；`managedRuntimeFeedTransport.ts:26-49` 仍以浅层检查后
  `as AgentLiveEvent` 收口；
- `contracts_check` 不在 PR/cloud image gate。

因此 generated gate 需要同时验证 DAG 和真实 runtime decoder，不可只验证文件内容未漂移。

### 2.4 registration 可枚举，但当前不是统一的 reachability contract

可利用的 production registration：

- HTTP root 在 `agentdash-api/src/routes.rs:73-139` 集中 merge/nest；资源 route table 保留在各模块；
- MCP 使用 `#[tool_router]`，`Self::tool_router().list_all()` 可运行时枚举；
- Product Runtime tools 在 `app_state.rs:320-378` 先注册 deferred services，之后
  `:654-707,750-752` 再 install；
- Local Relay command variants 在 `agentdash-relay/src/protocol.rs:136-589`，Local handler 在
  `agentdash-local/src/handlers/mod.rs:125-263`；
- Tauri 25 个 command 在 `agentdash-local-tauri/src/main.rs:378-404` 集中注册，TS 调用散布于
  `packages/app-tauri/src/{runtimeApi,desktopSettings,App}.ts(x)`；
- API worker 在 `agentdash-api/src/bootstrap/background_workers.rs:10-83` 集中启动两个 loop。

现状不能证明 completeness：

- Axum root 没有 endpoint descriptor/owner ledger，也没有逐 method/path 的 production router smoke；
- MCP `list_all()` 仅有局部测试，未统一映射 capability/command owner；
- `DeferredProductRuntimeToolService::is_installed()` 存在
  (`runtime_tool_executors.rs:137-139`) 但 production 无 consumer；删一条 install 线不会在启动期失败；
- Local handler `other => vec![]` (`handlers/mod.rs:257-261`) 会让新增 `Command*` variant 静默忽略；
- `dispatch_plan_for_message` 缺分类时默认 INLINE (`:266-273`)；
- Tauri Rust/TS 两侧当前 command name 数量相符，但没有参数 casing、结果 DTO、decoder parity；
- worker 没有 required descriptor、start result、health handle 或“删除一条 start 线必失败”的测试。

reachability 不能只靠源码计数。最终应让 registration 本身产生 descriptor，再以行为测试证明它被真实
composition 消费。

### 2.5 migration history guard 在 PR CI 中可能空跑

`scripts/check-migration-history.js:37-39,77-82` 只读取 `git diff --cached`。本地 pre-commit staged
场景有效；GitHub Actions checkout 中通常没有 staged diff，`pr_quick` 调用它时可能得到空集合并通过。
`BASE_REF` 目前只用于允许“恢复到 base” (`:48-65,95`)，没有用于枚举 PR range。

另有两个问题：

- `ALLOW_MIGRATION_BASELINE_REWRITE=1` 是不带 owner/task/expiry 的全局 bypass (`:6,71-75`)；
- schema readiness 再次手写 57 个 required table 和大量 retired table
  (`agentdash-infrastructure/src/migration.rs:4-190`)，与 migration DDL 双录，且不表达 table owner/read/
  retirement contract。

因此 migration history、effective schema 和 data ownership 必须拆成三个独立但关联的 gate。

## 3. 权威输入模型

建议新建 `architecture/` 下的 machine manifests；使用 JSON/JSON Schema 可以直接由 Node/Rust/TS
消费，避免另建 TOML parser。文件可以拆分，但共享一个 schema version：

| 输入 | 唯一拥有的事实 | 不应复制的事实 |
| --- | --- | --- |
| Cargo metadata | Rust workspace member、manifest path、production/dev edge | layer 与业务 owner |
| pnpm recursive list + package.json | pnpm package root、name、workspace dependency、exports | feature/store owner |
| `architecture/packages.json` | package layer、subsystem、production/example 分类、允许 layer edge | Cargo/pnpm edge本身 |
| `architecture/public-owners.json` | public contract symbol/module 的唯一 owner、允许 re-export | Rust/TS type定义 |
| code-owned entrypoint descriptor + `architecture/capabilities.json` | registration 事实 + capability/command owner 分类 | route/tool/command实现 |
| migration DDL + `architecture/data-owners.json` | effective table集合 + aggregate/read/retire/effect owner | DDL字段、repository逻辑 |
| generator CLI + `architecture/contracts.json` | owner/input/output/depends_on/check/runtime codec 分类 | DTO/schema内容 |
| subsystem builder/worker registry | required binding、worker、callback、freeze条件 | service业务实现 |
| `architecture/frontend-owners.json` | feature/store/dispatcher/presentation port owner 与允许 import | Zustand state shape |
| `architecture/exceptions.json` | 精确、临时、可删除的过渡例外 | 当前违规 baseline |

### 3.1 root discovery 与 missing-root 规则

architecture harness 的 root discovery 必须是双向的：

1. `cargo metadata` 的每个 workspace member必须恰好有一条 package classification；
2. package classification 指向的 Rust package必须存在于 metadata；
3. `pnpm -r list --depth -1 --json` 的每个 package必须分类为 production/shared/app/example；
4. pnpm classification 指向的 package必须真实存在；
5. frontend feature owner从 production package root下的 `src/features/*` 自动枚举，新目录未分类失败；
6. generated output/input、data table、entrypoint handler、public owner symbol均必须解析；
7. active architecture docs只引用 generated workspace inventory。README/spec 中出现的 current-crate token
   必须能解析到 workspace member；任务 research/archive 不参与当前架构事实检查。

不存在“root 不存在则 skip”。统一错误码：

```text
ARCH-ROOT-001 missing-root kind=rust-package ref=agentdash-executor
declared_by=architecture/packages.json#/packages/...
observed_workspace=Cargo.toml
remediation=删除失效声明，或将真实 package 加回 workspace；不得缩小扫描范围绕过
```

### 3.2 文档投影

当前 crate/package inventory 应由 harness 生成 `architecture/workspace-inventory.md`，active spec 和
README 只链接或嵌入带 digest 的 generated block。`architecture:check` 在内存中重渲染并 byte-compare。
这样：

- workspace 增删要求更新 machine classification；
- generated docs 随 classification 更新；
- guard扫描根仍来自 workspace，不来自文档；
- 文档不能反过来定义一个不存在的 crate。

不建议扫描所有历史任务中的 crate 名称；历史材料可以合法描述旧结构，不属于 active architecture。

## 4. 可执行规则矩阵

### G0 — Harness、workspace 与文档自一致

- **权威输入**：Cargo metadata、pnpm recursive list、`architecture/packages.json`、active generated doc。
- **检查形态**：Node orchestration + Rust/TS parser子检查；双向 package classification；generated doc
  byte compare；所有 root/symbol严格解析。
- **当前接入点**：`scripts/lib/quality-gates.js` 新增 `architecture_manifest_check`、
  `architecture_self_test`；`package.json` 新增 `architecture:check/self-test`。
- **CI gate**：`pr_quick` 第一段；修改 Cargo/pnpm/architecture/spec/README 时必跑。
- **self-test**：
  - valid：一 Rust + 一 pnpm fixture 都被分类，doc digest一致；
  - invalid：新增未分类 workspace member，期望 `ARCH-WORKSPACE-002 unclassified-member`；
  - missing-root：manifest引用已删除 package，期望 `ARCH-ROOT-001`。
- **失败消息**：必须包含 rule、manifest pointer、observed member/root、修复方向。
- **例外**：无。root 与 manifest 漂移不能豁免。
- **当前状态**：不存在；active docs/background guard已有真实 drift。

### G1 — crate/package 依赖方向

- **权威输入**：Cargo/pnpm 实际 graph + `packages.json` layer/subsystem + exact allowed-edge rules。
- **检查形态**：
  - `cargo metadata --no-deps --format-version 1` 分 production/build/dev graph；
  - pnpm graph从 package.json workspace dependency解析；
  - layer矩阵先判断，少量合理 composition edge以 exact package pair声明；
  - SCC只作附加错误，不能替代 direction。
- **必须阻断**：
  - domain/foundation -> application/infrastructure/interface；
  - persistence -> application use-case/API/Host/Integration；
  - `platform-spi -> agentdash-agent` implementation；
  - package private source跨包相对 import；
  - production edge借 dev dependency逃逸。
- **当前接入点**：现有 `backend_check/shared_check` 之前运行。
- **CI gate**：`pr_quick` + cloud image；完整 production+dev graph进入 architecture static。
- **self-test**：
  - valid：interface -> application -> domain、composition -> adapter；
  - invalid：foundation fixture依赖 implementation；
  - missing-root：edge rule任一 endpoint不是 workspace package。
- **失败消息**：
  `ARCH-DEP-101 forbidden-edge from=agentdash-platform-spi(layer=foundation)
  to=agentdash-agent(layer=implementation) reason=foundation_must_not_depend_on_implementation`。
- **例外**：只允许 exact edge、owner、关联 in-progress work package、最迟日期；禁止 wildcard/layer-wide。
- **当前违规**：既有研究已证实 platform-spi、infrastructure、application aggregate 方向问题。
- **完成条件**：相应 hard-cut package 全绿后同一个 PR 启用 blocking rule，不保留 baseline。

### G2 — public API ownership 与 source boundary

- **权威输入**：Rust/TS AST、package exports、`public-owners.json`。
- **检查形态**：
  - Rust AST检查 `pub use`、public field/parameter/return的类型 owner；
  - TS AST解析 import resolution，禁止跨 package private source；
  - source semantic rules检查 `RepositorySet`、`state.repos`、route中的 `Postgres*` concrete；
  - `RepositorySet` 允许位置从 owner symbol/role声明，不以目录 glob allowlist表达。
- **当前接入点**：替代/吸收孤立 source guards；`check-test-support-boundaries.js` 的位置检查可先迁入，
  adapter conformance另走 behavior gate。
- **CI gate**：`pr_quick/architecture_static`。
- **self-test**：
  - valid：route只依赖 application handle、SPI只 re-export 自有 contract；
  - invalid：foundation public API暴露 implementation type，或 route import Postgres concrete；
  - missing-root：public owner symbol/module无法由 AST/rustdoc解析。
- **失败消息**：
  `ARCH-PUBLIC-201 owner-leak public=agentdash_platform_spi::ExecutionTurnFrame
  leaked_type=agentdash_agent::DynAgentTool expected_owner=agentdash-agent-api`；
  `ARCH-SOURCE-202 service-locator file=... symbol=RepositorySet allowed_role=composition-only`。
- **例外**：public contract owner变化必须走同一 hard cut；不允许 compatibility re-export 例外。
- **静态边界**：AST可以证明依赖/泄漏；它不能证明 port语义正确，后者由行为测试负责。

### G3 — route/tool/worker/Relay/Tauri reachability

- **权威输入**：code-owned registration descriptor + `capabilities.json` 的 capability/command owner映射。
- **长期实现形态**：
  1. HTTP route模块用同一 table/macro同时生成 `Router` 和
     `{method,path,capability_id,auth_kind,handler}` descriptor；
  2. root只 merge `RouteContribution`，harness枚举所有 contribution；
  3. MCP 从 `Self::tool_router().list_all()` 读取真实工具名并映射 capability owner；
  4. Runtime tool builder公开 definition/executor/required binding descriptor；
  5. worker通过 `WorkerContribution { id, owner, start, health }` 注册；
  6. Relay 将 command 从通用 message拆成可枚举 command kind，Local match不允许 wildcard；
  7. Tauri command manifest从 Rust owner生成 typed invoke client。
- **过渡检查**：在宏/table改造前，AST比较 `routes/*.rs` 的 `.route` 与 root `.merge`，比较
  `RelayMessage::Command*` 与 Local handler arms，比较 Tauri `generate_handler!` 与 TS invoke names。
  过渡检查只能防漏注册，不能作为最终 DTO/行为保证。
- **行为 gate**：
  - 每个 HTTP descriptor以 production router `oneshot` 发请求，断言不是 404/405；鉴权拒绝也证明可达；
  - 每个 MCP tool真实出现在 list，并至少有 command-owner spy验证；
  - Runtime tool `freeze()` 后每个 required kind均有 executor；
  - 每个 required worker返回 handle/health，删除 start contribution会使 bootstrap test失败；
  - 每个 Relay command decode后进入唯一 Local handler；
  - Tauri packaged smoke调用 snapshot/settings/browse/update。
- **当前接入点**：`routes.rs`、RMCP tool router、`PlatformToolBroker`、background worker bootstrap、
  Local command router、Tauri `generate_handler!` 都可改造成 descriptor producer。
- **CI gate**：静态分类进 `pr_quick`；HTTP/MCP/Runtime/worker/Relay行为进
  `architecture_integration`；Tauri packaged smoke进 Windows desktop gate。
- **self-test**：
  - valid：每类各一条 descriptor/handler/owner完整；
  - invalid：注册 endpoint 无 capability、capability无 production registration、Relay command无 handler；
  - missing-root：descriptor handler symbol、owner package或worker factory不存在。
- **失败消息**：
  `ARCH-REACH-301 unclassified-entrypoint kind=relay command=command.tool.search`；
  `ARCH-REACH-302 unreachable-entrypoint kind=http method=POST path=/api/... observed=404`；
  `ARCH-REACH-303 missing-worker-binding worker=workflow_recovery owner=workflow`。
- **例外**：dead code应删除；不能以“暂未挂载”进入 production ledger。

### G4 — composition completeness

- **权威输入**：subsystem builder 的 required key集合与实际 binding，不另建手工服务清单。
- **检查形态**：
  - builder协议统一为 `declare -> bind -> validate/freeze`；
  - `freeze()` 返回不可变 handle；Router/Broker/worker只能接收 frozen handle；
  - required binding使用 typed key/enum，不靠 string；
  - deferred placeholder不能进入 frozen catalog。
- **当前接入点**：替换 `DeferredProductRuntimeToolService` 的注册后 install时序；同一模式覆盖 Hook
  callback、Operation gateway、provider、registry和worker。
- **CI gate**：定向 composition tests进入 `architecture_integration`，不是只进手工 heavy。
- **self-test**：
  - valid：完整 bindings freeze成功并可调用；
  - invalid：删除任一 required binding，freeze返回列出全部 missing keys的错误；
  - missing-root：required key无任何 provider/contribution声明。
- **失败消息**：
  `ARCH-COMP-401 incomplete-subsystem subsystem=runtime_tools
  missing=[complete_lifecycle_node] bound=[...]`。
- **例外**：optional必须由业务 contract明确为 optional，并有 absence behavior test；不能把 required
  改成 optional来通过门禁。
- **必须行为测试的原因**：源码能看到 `.install()` 不代表真实构造路径一定执行；只有 production builder
  的 freeze/smoke 能证明。

### G5 — semantic transaction 与 data owner

- **权威输入**：
  - migration解析出的 effective table/object集合；
  - `data-owners.json` 的 aggregate owner、semantic command port、authoritative read、retire/recovery；
  - production transaction port与 failure-injection conformance suite。
- **静态检查**：
  - 每张 table/document/object prefix恰好一个 owner；
  - ledger无孤立或多 owner；
  - 声明的 owner package/port/test ID均可解析；
  - capability写多个 owner或含 database+object/process/relay effect时，必须声明 consistency strategy：
    `single_transaction | durable_intent_receipt_replay`；
  - Project-owned新 table必须进入 retirement contract。
- **行为检查**：
  - transaction在第 N 个写入点故障时全部回滚；
  - external effect在 receipt丢失、重复 claim、restart时可重放且不重复业务结果；
  - authoritative read能读回 committed fact；
  - retirement对新增 owner fact和object cleanup闭合。
- **当前接入点**：Interaction/Workflow现有 semantic transaction tests可作为 conformance模式；
  Project deletion、Shared Library compound install作为首批失败注入对象。
- **CI gate**：静态 owner分类进 PR quick；定向 PostgreSQL/failure injection进 architecture integration。
- **self-test**：
  - valid：两表同事务 + readback；
  - invalid：fixture第二写失败后第一写仍存在，conformance suite必须失败；
  - missing-root：DDL出现未分类 table，或 ledger声明不存在 table/port。
- **失败消息**：
  `ARCH-DATA-501 unowned-table table=foo source=000x.sql`；
  `ARCH-TXN-502 partial-commit capability=project_retirement injected_step=3
  surviving_fact=project_agents/...`。
- **例外**：P0 authority/transaction规则不允许例外。跨系统确实不能单事务时必须实现 durable intent/
  receipt/replay，而不是声明 eventual consistency。
- **观察指标**：SQL insert/select引用计数只能定位 write-only候选，不能作为“已有 authoritative reader”
  的证明。

### G6 — migration history、effective schema 与 owner同步

- **权威输入**：Git base/range、migration目录、SQLx effective migration、data owner ledger。
- **检查形态**：
  - 本地 hook明确使用 `--mode staged`；
  - CI明确传 `--mode range --base $PR_BASE_SHA --head $GITHUB_SHA`；
  - base ref无法解析时失败，不能返回空 diff；
  - 已在 base存在的 migration被修改/删除/重命名时失败；
  - 新 migration应用到空库和代表性开发库快照；
  - effective `pg_catalog` table集合与 data owner ledger双向一致；
  - required readiness从声明式 owner/schema inventory派生，停止手写全表双录。
- **当前接入点**：重写 `migration:guard` CLI但保留 package script/gate name；GitHub checkout必须获取
  足够 base history。
- **CI gate**：history静态检查在 `pr_quick`；apply/readiness/owner在 architecture integration。
- **self-test**：
  - valid：新增下一号 migration通过；
  - invalid：修改 base中 migration失败；
  - missing-root：base ref或 migration root不存在必须失败，不能视为空集合。
- **失败消息**：
  `ARCH-MIG-601 missing-base ref=... mode=range`；
  `ARCH-MIG-602 immutable-migration-rewritten path=... base=<sha> head=<sha>`。
- **例外**：移除无 owner 的通用环境变量 bypass。pre-release baseline rebuild是独立、显式授权的
  maintenance work package，使用 exact path/digest/task/expiry的一次性授权，并在任务结束时清零；
  release attestation不接受存量例外。

### G7 — generated contract DAG 与 runtime decoder

- **权威输入**：`contracts.json` 的 generator owner/input/output/depends_on/check/runtime_codec分类；
  实际 generator/check CLI；Rust serialization fixtures。
- **静态检查**：
  - 每个 output唯一 generator；
  - input/output root必须存在且属于声明 owner；
  - DAG无环并按拓扑执行；
  - 跨 output import必须有对应 `depends_on`；
  - check mode只读；missing/outdated/stale output失败；
  - 每个 browser/desktop NDJSON/HTTP/IPC boundary必须声明 generated decoder及consumer；
  - transport中的 `as GeneratedWireType`、逐字段 identity rebuild、重复 enum policy进入 source guard。
- **行为检查**：
  - Rust serialize fixture -> TS decoder -> encoder/consumer roundtrip；
  - valid branch、unknown discriminant、valid discriminant+invalid nested payload、u64边界；
  - Agent live decoder覆盖完整 `AgentLiveEvent`/canonical record，不只顶层；
  - generator输入变更但输出未更新时 gate失败。
- **当前接入点**：把现有六段 `contracts:check` 迁为由 DAG manifest拓扑执行；保留各 generator内的
  required declaration/collision tests。
- **CI gate**：`contracts_check`进入 `pr_quick` 与 cloud image；decoder fixture进入
  `architecture_contract`；deployment/image记录 contract manifest+output digest。
- **self-test**：
  - valid：A -> B DAG、唯一 outputs、decoder fixture通过；
  - invalid：duplicate output、cycle、drift、坏 nested payload分别失败；
  - missing-root：generator owner/input/output/decoder任一缺失失败。
- **失败消息**：
  `ARCH-CONTRACT-701 duplicate-output output=... owners=[A,B]`；
  `ARCH-CONTRACT-702 cycle path=A->B->A`；
  `ARCH-CODEC-703 invalid-wire contract=AgentLiveEvent path=$.record... expected=...`。
- **例外**：内部跨进程合同不能用手写 mapper例外；外部 vendor payload可以由 integration adapter拥有
  decoder，但仍需在 manifest中分类为 external。

### G8 — frontend owner-scoped dispatcher/store

- **权威输入**：TS import graph、`frontend-owners.json` 的 feature/store/dispatcher/presentation port owner、
  generated event union。
- **静态检查**：
  - 新 `src/features/*` 必须分类；
  - foreign feature不能 import store implementation或调用其 `.getState()`；
  - raw Project/Runtime event只有 declared dispatcher可消费；
  - App composition只 wire transport -> dispatcher，不解释业务 discriminant；
  - extension/canvas/iframe/renderer只能调用 workspace-scoped presentation port；
  - app-tauri只能依赖 app-web package exports，禁止 `app-web/src/**` 私有源码相对 import。
- **行为检查**：
  - generated union每个 variant恰好映射到 owner-scoped typed effect；
  - target/project/workspace隔离；
  - old iframe callback不能写入新 workspace；
  - dispatcher重复输入去重且不会重复刷新；
  - store最终持久化结果与 presentation port意图一致。
- **当前接入点**：`packages/app-web/eslint.config.js` 目前没有 architecture import rule；可由统一 TS AST
  harness实现，并在 ESLint中保留局部语法规则。现有 `workspaceTabStore` stable operation可作为目标 port。
- **CI gate**：AST check进 PR quick；Vitest组合测试进 architecture contract/integration。
- **self-test**：
  - valid：feature只 import typed owner port；
  - invalid：extension直接 import global tab store，或 App switch raw event；
  - missing-root：manifest owner/store/dispatcher path不存在。
- **失败消息**：
  `ARCH-FE-801 foreign-store-access importer=extension-runtime target=workspaceTabStore
  required_port=WorkspacePresentationPort`；
  `ARCH-FE-802 raw-event-owner importer=App.tsx expected=ProjectEventDispatcher`。
- **例外**：composition owner可以持有 port引用但不能拥有业务 switch；不允许以 App 为全局 wildcard owner。

### G9 — Tauri/Relay/HTTP IPC 与跨平台 path

- **权威输入**：Rust-owned command/DTO/Relay contract、generated manifest/client/decoder、typed path
  presentation；不是 TS手写 interface。
- **静态检查**：
  - Tauri Rust manifest与 generated TS client method/argument/result一一对应；
  - 禁止 raw `invoke("...")` 出现在 generated adapter之外；
  - Relay command enum、cloud sender、Local handler、response kind全部分类；
  - HTTP/Tauri内部 DTO必须属于 generated contract；
  - shared view不能从裸 path string推断 OS grammar。
- **行为检查**：
  - packaged Tauri smoke至少 snapshot/settings/browse/update；
  - Relay每个 command encode/decode/dispatch/response；
  - Windows drive、UNC、POSIX root/child breadcrumb roundtrip；
  - malformed IPC response由 decoder在 adapter边界失败。
- **当前接入点**：25 command集中 `generate_handler!`，具备生成/比较入口；Relay enum和Local router可拆成
  typed exhaustive command path；Directory Browser是 path matrix目标。
- **CI gate**：name/type parity进 PR quick/desktop check；Windows packaged smoke与path matrix进
  desktop workflow；HTTP/Relay codec进 architecture contract。
- **self-test**：
  - valid：一条 generated command和三种 path style roundtrip；
  - invalid：TS多/少 command、参数 casing漂移、POSIX root丢失；
  - missing-root：Rust command owner、TS generated package或path style variant缺失。
- **失败消息**：
  `ARCH-IPC-901 command-parity command=runtime_start mismatch=result`；
  `ARCH-PATH-902 non-roundtrip style=posix input=/home/user output=home/user`。
- **例外**：外部 OS API可在 adapter内保留平台 shape；shared/product contract不允许裸 string推断例外。

## 5. 静态检查、行为测试、观测指标的边界

| 类别 | 可以得出的结论 | 不能冒充的结论 |
| --- | --- | --- |
| workspace/dependency/AST static | member已分类、依赖方向、public type owner、import/source禁区、输出唯一、
  entrypoint有分类 | transaction原子、route真实可调用、worker可恢复 |
| generated drift/schema static | output与当前 generator一致、DAG完整、decoder被声明/消费 | decoder覆盖全部坏数据语义 |
| router/tool/composition behavior | production registration可达、required binding缺失会失败 | 重启/并发/部分失败后仍收敛 |
| failure-injection/recovery integration | rollback、receipt replay、restart、owner read真实闭环 | 所有未来业务路径自动满足，仍需 ledger完整性 |
| desktop/path packaged matrix | Rust/TS invoke与平台 path真实闭环 | 未测试平台一定正确 |
| metrics | fanout/churn、raw subscriber数量、worker lag、gate耗时、异常频率 | owner正确、边界稳定、规则已完成 |

只能观测、不作为完成 gate 的指标：

- crate/package fan-in/fan-out、commit co-change、P90/P95 blast radius；
- `RepositorySet`/raw event/global store引用数在 hard cut前的收敛趋势；
- generated output数量、route/tool/worker数量本身；
- architecture gate耗时与 flaky rate；
- worker recovery lag、decoder rejection rate、unknown command telemetry。

以下必须为零且属于 gate，不是指标：

- unclassified/stale/missing root；
- forbidden dependency/public leak；
- unowned table/output/entrypoint；
- missing required binding/worker；
- contract drift/duplicate output/DAG cycle；
- foreign store/raw event bypass；
- Tauri/Relay command parity缺口；
- 有效期已过或无 task的 exception。

## 6. Exception 模型

`architecture/exceptions.json` 仅用于跨 PR hard cut的短暂过渡，不用于把当前违规固化成 baseline：

```json
{
  "id": "ARCH-EX-...",
  "rule_id": "ARCH-DEP-101",
  "exact_subject": "agentdash-x -> agentdash-y",
  "owner": "team-or-module-owner",
  "rationale": "稳定边界变更为何必须跨两个原子提交",
  "task_path": ".trellis/tasks/...",
  "expires_on": "YYYY-MM-DD",
  "removal_condition": "第二个 package 删除 exact edge"
}
```

validator要求：

- rule/subject必须 exact，禁止 glob、regex、layer-wide；
- target/root当前必须存在；
- task必须存在且处于 active/in_progress；
- 到期日有最大窗口并由 CI当前日期检查；
- authority/transaction/data loss、missing-root、contract drift不允许 exception；
- 每个 work package完成定义要求删除自己引入的 exception；
- cloud image/release attestation要求 exception集合为空。

因此 exception通过时仍是明确、可阻断到期的临时授权，不是 warning，也不是永久 allowlist。

## 7. Gate 拓扑与 CI 接入

建议最终 gate：

| Gate | 内容 | 现有接入 |
| --- | --- | --- |
| `architecture_self_test` | 每条规则 valid/invalid/missing-root fixtures；quality gate manifest tests | 新增；
  由 `pr_quick`先运行 |
| `architecture_static` | G0/G1/G2、entrypoint分类、data/contract/frontend/IPC静态 parity | 新增；
  复用 Node runner |
| `architecture_contract` | generator DAG/check、Rust fixture↔decoder、HTTP/Relay/Tauri parity | 从现有
  `contracts_check`扩展 |
| `architecture_integration` | route/tool/worker reachability、composition freeze、transaction/recovery、
  schema apply/readback | 新增定向 blocking CI；不能只放 manual heavy |
| `desktop_check` | 现有 typecheck/cargo check + generated IPC parity/path unit matrix | 扩展现有 gate |
| `desktop_packaged` | Windows packaged invoke smoke、UNC/drive/POSIX contract | 新增 Windows workflow |
| `cloud_image_preflight` | 强化后的 pr_quick + contract attestation | 继续复用现有 gate |
| `deployment_contract` | 校验 image内 architecture/contract attestation digest与commit一致 | 扩展现有 gate |

`quality-gates.test.js` 必须更新并进入 `architecture_self_test`，断言：

- 每个 required gate存在且非空；
- `pr_quick`包含 architecture self/static/contracts；
- cloud image复用强化后的 gate；
- desktop packaged workflow委托 manifest runner；
- workflow没有重写 gate命令；
- active spec/README变化能触发 architecture检查。

branch protection/required check是 GitHub外部状态，本研究无法验证；实施 WP 必须把新增 workflow/check name
交给仓库管理员设为 required，并在验收中记录实际 required-check截图或 API结果。

## 8. 启用顺序与后续 work packages

### WP-A0 — Architecture harness 与 fixture contract

- 建立 machine manifest schema、workspace-derived root discovery、结构化错误码；
- 建立每条 guard的 valid/invalid/missing-root fixture runner；
- 接入 quality gate manifest tests；
- 生成 current workspace inventory并修复 active docs/root drift；
- 此时 checker可以存在但对尚未 hard cut的规则不算完成。

验收：G0 blocking；不存在 silent skip；删除 fixture root必失败。

### WP-A1 — Migration range guard 与 schema owner bootstrap

- 将 migration guard拆成 staged/range模式；
- CI显式 base/head，missing base失败；
- 移除无 owner的通用 bypass；
- 从 effective DDL生成 table inventory，57个当前 table全部分类；
- readiness由 inventory派生。

验收：valid append、illegal rewrite、missing-base、unowned/stale table fixtures全绿；PR CI真实 diff可被制造失败。

### WP-A2 — Cargo/pnpm direction 与 public/source hard cut

依赖业务整改：

1. Platform SPI concrete Agent hard cut；
2. Persistence 与 runtime composition split；
3. application aggregate retirement；
4. RepositorySet/AppState route concrete收口。

每个子任务末尾依次启用对应 exact rule；最后启用完整 layer matrix与public owner gate。

验收：当前 graph零违规；compatibility re-export、route Postgres import、application RepositorySet fixture必失败。

### WP-A3 — Entrypoint descriptor 与 production reachability

拆为可独立验收的子包：

1. HTTP RouteContribution/table + production router smoke；
2. MCP tool capability/command-owner registry；
3. Relay exhaustive command split，删除 wildcard/default-inline；
4. worker contribution/health registry；
5. Runtime tool composition builder/freeze；
6. Tauri command manifest先完成 name reachability，DTO generation由 WP-A7继续。

验收：backend entrypoint coverage index中的 HTTP/MCP/tool/29 Local/25 Tauri/background类别均由真实
registration自动枚举；删任一 root registration/binding会失败。

### WP-A4 — Semantic transaction 与 data owner conformance

依赖业务整改：

1. Project retirement semantic port + object cleanup intent/receipt/replay；
2. Shared Library AgentTemplate+MCP compound install transaction；
3. 其它 capability ledger中多 owner写入逐项补 strategy/failure suite；
4. repository memory/Postgres conformance按 owner执行。

验收：每个跨 owner mutation都有 failure matrix；P0规则无 exception；新增 Project-owned table未进入
retirement contract必失败。

### WP-A5 — Generated DAG 与 runtime decoder

- manifest化六个 generator及全部 input/output/depends_on；
- 修正 check mode只读；
- 将 `contracts:check`改为拓扑执行；
- 统一 JSON integer policy；
- 生成完整 AgentLiveEvent decoder与 Rust fixtures；
- 收口 Project Agent/File Picker/Browse/Terminal等 route-local wire DTO。

验收：duplicate/cycle/missing output/drift/坏 nested payload/u64边界全部有负向 fixture；PR/cloud image
blocking。

### WP-A6 — Frontend dispatcher 与 owner store

- 建立 frontend owner manifest和TS AST import graph；
- Project raw event收口到单一 dispatcher；
- extension/canvas/tab producer收口 WorkspacePresentationPort；
- foreign store access hard cut；
- App只保留 wiring；
- owner-scoped dispatcher/store组合测试。

验收：当前 direct global tab store/raw Project bus多 consumer清零后启用 G8 blocking；不保留基线。

### WP-A7 — Desktop/Relay/path generated contract

- Rust Tauri contract生成共享 TS package/client/decoder；
- app-tauri删除 app-web私有 generated源码 import；
- Relay command/response codec与 Tauri manifest纳入 contract DAG；
- backend返回 typed path presentation，shared view不推断 OS grammar；
- 新增 Windows desktop packaged workflow。

验收：25个 command parity、packaged smoke、Windows drive/UNC/POSIX roundtrip全绿；raw invoke与裸 path
parser负向 fixture失败。

### WP-A8 — CI/release attestation hardening

- `pr_quick`加入 self/static/contracts；
- 新增 blocking architecture integration和desktop packaged workflow；
- cloud image生成包含 commit、architecture manifest digest、contract output digest、gate version的
  attestation；
- deployment contract验证 attestation；
- 确认 required checks已配置；
- exception集合清零。

验收：普通 PR、cloud image、desktop release、deployment不存在绕过 architecture contract的路径。

### 依赖图

```text
WP-A0
  ├─> WP-A1
  ├─> WP-A2
  ├─> WP-A3 ──> WP-A4
  ├─> WP-A5
  └─> WP-A6
WP-A3 + WP-A5 ──> WP-A7
WP-A1..A7 ──────> WP-A8
```

WP-A0只建立可信 checker/self-test，不把尚未扫描 production的规则标为完成。WP-A1可独立先修当前
migration假门禁；WP-A2～A7按各自 hard cut消除当前违规，并在同一 work package末尾启用 blocking
rule；WP-A8只负责闭合所有发布入口，不承担替业务整改建立 baseline。

## 9. 失败输出规范

所有 checker输出单行机器可解析摘要，随后给证据：

```text
<RULE_ID> <short-kind> subject=<...> owner=<...>
declared=<manifest json pointer or code descriptor>
observed=<actual edge/root/path/binding/payload>
remediation=<唯一正确的 owner/boundary 修复方向>
```

要求：

- 缺 root与“零发现”严格区分；
- 输出 violated edge、owner和实际证据文件/行；
- 不输出“加入 allowlist即可”；
- transaction/decoder行为失败包含 injected step或 JSON path；
- composition失败一次列完全部 missing keys，避免逐次修一个；
- CI artifact保存 JSON report和manifest digest，console保留短摘要。

## 10. 研究判断

### 可以立即落地

- workspace-derived root discovery、machine manifest schema、missing-root self-tests；
- Cargo/pnpm依赖方向；
- public re-export/RepositorySet/route concrete/跨包私有 import AST规则；
- migration staged/range模式；
- generator DAG唯一 output/环/缺 root/drift；
- Tauri/Relay/HTTP registration静态分类；
- frontend foreign store/raw event owner import规则；
- quality gate/CI membership tests。

### 必须随业务 hard cut落地

- subsystem freeze；
- route/tool/worker真实 production reachability；
- Project/Shared Library semantic transaction；
- data authoritative reader/retirement；
- Agent live完整 decoder；
- owner-scoped dispatcher；
- generated Tauri DTO/client；
- typed cross-platform path。

### 不能靠静态 guard完成

- transaction rollback与external effect replay；
- worker restart/recovery；
- production route鉴权/owner语义；
- dispatcher target隔离和最终 store持久化；
- packaged Tauri invoke；
- Windows/UNC/POSIX真实 roundtrip。

这些必须进入 blocking behavior/integration gate；静态“存在某个函数/测试文件”只能检查 suite注册完整，
不能替代执行。

## Files Found

| 文件 | 一句话说明 |
| --- | --- |
| `scripts/lib/quality-gates.js:1-166` | 当前 gate/step 单一 manifest和实际 coverage |
| `scripts/quality-gates.js:35-125` | gate runner、dry-run与expect-failure入口 |
| `scripts/lib/quality-gates.test.js:16-174` | gate组成、root scripts与workflow委托测试，当前未入 gate |
| `package.json:51-63` | guard、contract generator顺序与本地质量门命令 |
| `.github/workflows/{pr-quick,heavy-check,cloud-image,deploy-contract}.yml` | 当前四个 CI入口 |
| `Cargo.toml:1-45,103-145` | 40个 Rust workspace member与内部 dependency registry |
| `pnpm-workspace.yaml:1-3`、`packages/*/package.json` | pnpm root、package exports和workspace edge |
| `scripts/check-background-process-spawn.js:11-93` | stale scan root、missing-root silent skip且未接 gate |
| `scripts/check-test-support-boundaries.js:6-89` | 全 crates扫描但依赖命名正则的 source guard |
| `scripts/check-migration-history.js:5-116` | staged-only history guard与无 owner bypass |
| `crates/agentdash-infrastructure/src/migration.rs:4-190` | required/retired schema双录 |
| `crates/agentdash-*/src/generate*.rs` | 六段 generator/check mode、跨 output读取和局部 self-test |
| `packages/app-web/src/generated/ndjson-stream-validators.ts` | 已生成 Project/Session NDJSON validator |
| `packages/app-web/src/generated/agent-service-codecs.ts` | Service API codec，当前无 AgentLiveEvent decoder |
| `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedTransport.ts:26-49` | 浅层检查后 cast AgentLiveEvent |
| `crates/agentdash-api/src/routes.rs:55-142` | HTTP/MCP production root composition |
| `crates/agentdash-api/src/app_state.rs:320-378,654-707,750-823` | deferred Runtime tool注册/安装与worker启动时序 |
| `crates/agentdash-infrastructure/src/runtime_tool_executors.rs:109-163` | deferred service、未使用的 completeness probe |
| `crates/agentdash-api/src/bootstrap/background_workers.rs:10-83` | 两个无 descriptor/health handle的production worker |
| `crates/agentdash-relay/src/protocol.rs:136-589` | Local Relay command事实源 |
| `crates/agentdash-local/src/handlers/mod.rs:125-273` | command handler wildcard与default-inline gap |
| `crates/agentdash-local-tauri/src/main.rs:378-404` | 25个 Tauri command registration |
| `packages/app-tauri/src/{runtimeApi,desktopSettings,App}.ts(x)` | 手写 invoke client和command names |
| `packages/app-web/eslint.config.js` | 当前无架构 import/store owner规则 |

## Code Patterns

### Pattern A：guard root必须来自 workspace

手写 `scanRoots` 即使今天正确，crate rename后也会静默过期。正确模式是 metadata枚举 + manifest双向分类；
guard只接收 package identity，不接收目录猜测。

### Pattern B：registration descriptor必须和真实注册同源

第二份手写 endpoint/tool/Tauri清单只能制造新 drift。route macro、RMCP router、typed builder和 generated
Tauri client应同时产生可运行注册与 descriptor；capability ledger只补 owner分类。

### Pattern C：静态声明完整不等于 composition完整

`.install()`、worker spawn、route function和test文件存在都不能证明 production path执行。freeze、
oneshot、failure injection和packaged smoke必须运行。

### Pattern D：generated type不等于 runtime contract

TypeScript union可编译不代表网络 payload合法。每个跨进程 browser/desktop wire要有 generated decoder和
Rust fixture；transport只负责 framing/reconnect/error reporting。

### Pattern E：schema owner必须以真实读写行为收口

DDL parser能证明 table集合和分类，源码 grep只能发现无读者候选；authoritative read、rollback、retirement
与 replay必须由 PostgreSQL行为测试证明。

### Pattern F：CI gate本身也需要 self-test

`quality-gates.test.js` 不进入 gate时，只能说明测试文件存在。gate membership、workflow委托、missing
root和negative fixture都必须由一个更上层的 architecture self-test gate执行。

## External References

无。本研究只使用当前仓库代码、manifests、workflows、migration、tests、active specs与本任务已有
research；没有联网查询。相关本地版本为 Rust edition 2024、pnpm 11.9.0、TypeScript 5.9.3、Tauri
2.11.1。

## Related Specs

- `.trellis/spec/backend/architecture.md` — Interface/Application/Domain/SPI依赖方向、具名 use-case deps、
  route/composition边界。
- `.trellis/spec/backend/repository-pattern.md` — RepositorySet只用于 bootstrap/composition、跨聚合
  semantic port。
- `.trellis/spec/backend/database-guidelines.md` — PostgreSQL migration/schema authority。
- `.trellis/spec/backend/quality-guidelines.md` — test-support guard、RMCP tool router枚举方式。
- `.trellis/spec/frontend/architecture.md` — generated contract与frontend package/feature owner。
- `.trellis/spec/frontend/state-management.md` — typed dispatcher、workspace-scoped tab operation与owner read。
- `.trellis/spec/frontend/type-safety.md` — generated wire/runtime validator、禁止 identity rebuild。
- `.trellis/spec/cross-layer/architecture.md` — Rust contract -> generated TS、Cloud/Local/Desktop owner。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — generator/check mode与跨层 DTO事实源。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` — Tauri/Relay/Local command与path边界。
- `.trellis/spec/guides/cross-layer-thinking-guide.md` — production composition、continuation handle、dispatcher、
  migration/recovery纵向检查。

## Caveats / Not Found

1. 本研究没有修改或运行产品代码、generator、migration、Cargo/pnpm test；结论基于 source/manifests/
   workflow静态核验和已有测试锚点。
2. 没有检查 GitHub branch protection，因此不能断言当前 workflow是否 required；只能证明 workflow/gate
   自身的组成。
3. Axum Router没有现成稳定的完整 route introspection API被当前项目使用；最终 route reachability需要
   code-owned descriptor/table和 `oneshot` behavior test，不能仅靠 AST长期维持。
4. Rust public API owner检查的最终 parser实现需要在 WP-A0选定：`syn/cargo metadata` 自有工具、
   rustdoc JSON或等价稳定方案。正则只能作为过渡 fixture，不应成为终态。
5. pnpm workspace包含 `examples/extensions/*`；harness必须把它们显式分类为 example，而不是从扫描中
   排除，否则新增 example root同样可能静默。
6. data owner ledger可以证明分类完整，不能从静态 SQL引用证明“authoritative reader”真实可用；
   该项必须等待 WP-A4 PostgreSQL conformance。
7. 当前 Tauri 25个 Rust registration均能在 app-tauri找到相应 invoke意图，但本研究没有运行 packaged
   app；这不证明参数/返回/decoder parity。
8. 现有 Project/Session generated NDJSON validators说明 generator方向可复用；它们不覆盖 Agent live，
   也不能代表所有 validator已具备完整 nested semantic validation。
