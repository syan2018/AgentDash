# Main-Parity 会话链路恢复执行计划

## 1. 执行规则

- 只使用当前主任务，不创建child task。
- 当前实现基线为本任务重订提交，父分支基于`36c5484e6`；不得擅自继续回退已有protocol/runtime/frontend提交，所有恢复以main行为oracle逐项实现。
- main oracle固定为只读`D:\Projects\AgentDash-main-reference@957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- session presentation与AgentRun外层行为发生规范冲突时，以本PRD和main oracle为验收依据；W0记录冲突，W11更新最终spec。
- 每个工作项由implement agent实现、不同check agent复核、主会话审阅并单独提交。
- check失败时唤醒同一个implement agent修复，禁止换agent重新研究。
- 所有工作项直接在当前本地分支实施；禁止创建临时worktree或工作项branch。参考main worktree只读且不作为实现工作区。
- 依赖满足且ownership不重叠的工作项允许并行。每个派发prompt必须明确告知“共享工作区存在并行改动”，列出本项ownership paths，并要求保留、不得格式化或回退其他修改。
- 任一工作项需要修改别项ownership path时先停止并通知主会话，由主会话协调唯一owner和共享合同。
- 所有agent复用默认Cargo target/cache；Cargo锁竞争时正常等待，不另建target目录、不终止其他Cargo/rustc/rust-analyzer进程。构建排队不影响不需要该锁的工作并行。
- implement完成后由不同check agent检查该ownership diff；主会话只暂存该工作项路径并提交，其他并行修改保持未暂存。
- 不使用child status；进度只更新本文件checkbox、journal和逐工作项提交。

## 2. DAG 与合流门

```text
W0 Main Oracle/Harness ──┬── W1 Protocol 0.144.1
                         └─────────────┬── G1
                                       ▼
                              W2 Immutable Carrier
                                       ▼
                              W3 Persistence/Migration
                                       ▼
                    ┌──────────┬───────┴────────┬──────────┐
                    ▼          ▼                ▼          ▼
                W4 Codex  W5 Native/Remote  W6 Tools  W7 App Producers
                    └──────────┴───────┬────────┴──────────┘
                                       ▼ G3
                              W8 Journal/History/Stream
                                       ▼
                           ┌───────────┴───────────┐
                           ▼                       ▼
                  W9 features/session      W10 AgentRun Outer
                           └───────────┬───────────┘
                                       ▼
                              W11 Full Parity
```

并发调度：

- W0与W1可在共享工作区并行，但根Cargo/package manifest与lock只归W1；G1等待两项分别implement/check/commit。
- W2、W3是共享合同与存储基线，顺序完成。
- W4–W7在W3提交后可并行；按可用agent槽位分批启动，G3等待四项分别implement/check/commit。
- W8在W4–W7全部提交后完成最终journal/API接线。
- W9与W10在W8合同冻结后并行。
- W11只在所有工作项集成后运行。

合流门：

- **G0**：task文档、oracle commit、wrapper allowlist、单工作区ownership确认。
- **G1**：严格comparator负例通过；Codex`0.144.1`codegen/roundtrip通过。
- **G2**：完整presentation payload经过Runtime commit/persist/read/replay字节语义不变。
- **G3**：driver、Tool Catalog、application producer inventory均100%，不存在被过滤或反推的family。
- **G4**：journal/route、features/session、AgentRun outer三份main差异矩阵清零。
- **G5**：W11全链路eventstream与browser parity通过。

## 3. 共享工作区与ownership

所有W0–W11工作项都在`D:\Projects\AgentDash`当前分支执行，不为工作项创建git worktree、临时branch或独立Cargo target。已有`D:\Projects\AgentDash-main-reference`只作为固定main oracle读取，不承载实现修改。

并行agent共享同一个working tree；每次派发都要显式告知其当前存在并行改动，并限定ownership paths。Agent只读取全局状态、修改自己的范围，不运行跨仓库format/bulk rewrite，不清理、暂存、提交或回退他人文件。主会话负责按路径审阅、暂存与逐项提交；一个工作项提交导致HEAD前进时，其余agent继续保留当前未提交修改，不做rebase/reset。

Cargo、rustc和rust-analyzer共享默认build directory。锁被占用时等待即可，不把等待解释为失败，不终止占锁进程，也不切换到新的`CARGO_TARGET_DIR`。前端与无需Cargo锁的检查可以继续并行。

## 4. W0 — Main Oracle、行为账本与严格判定器

### Ownership

- `agentdash-test-support`/`agentdash-agent-runtime-test-support`中的session parity测试支持。
- 专用`session-parity`脚本、fixtures与本任务research账本。
- 不修改protocol generated产物、Runtime production contract、connector/API/frontend生产代码。
- 根Cargo/package manifest与lock由W1拥有；W0优先复用现有依赖，确需新增依赖时先由主会话协调，不能并行写同一文件。

### Scope

- 固定main/current commit与reference worktree preflight。
- 建立`BackboneEvent`、`PlatformEvent`、driver、Tool Catalog、application producer、route/service/frontend逐项inventory。
- 为main deterministic场景生成有provenance的golden fixtures：oracle commit、scenario input、固定clock/IDs、capture harness hash。
- main capture直接读取固定`D:\Projects\AgentDash-main-reference`；golden与harness只写当前工作区。若必须运行main侧构建，显式复用当前工作区的Cargo target/cache，遇锁等待，不能创建第二套target目录。
- 实现typed wrapper unwrap与ordered presentation comparator；protected body只取`notification.event`或等价`presentation_event`。
- 为event缺失/增加/重排、ID变化、timestamp变化、数字/字符串变化、null↔omitted、数组重排建立必失败负例。
- 建立route ledger、frontend file ledger与browser scenario ledger。

### Required fixture matrix

- Input：prompt/steer；Text/Image/LocalImage/Skill/Mention；system/workflow/companion delivery。
- Turn/message：started/completed/failed/interrupted/rewind；assistant；reasoning text/summary；plan/diff。
- Item/tool：started/updated/completed；command/file/MCP/Shell/Fs*/Dynamic及产品工具实际main表达。
- Status：usage/context、provider全阶段、error、thread status/title、compaction、hook、terminal/PTY/control-plane。
- Interaction：command/file/user-input/permission/MCP/dynamic request与resolution，按各connector main行为分别建fixture。
- Journal：GET/initial stream/live/reconnect/refresh/fork inherited/heartbeat/lagged/closed。
- AgentRun/frontend：submit、无phantom tool、refresh、fork、mailbox、context、approval、lineage、status bar。

### Exit criteria

- [x] 每个`BackboneEvent`和`PlatformEvent`variant都有owner或明确non-production证明。
- [x] 每个生产driver/tool/application producer都映射到fixture。
- [x] comparator所有负例按预期失败，不存在payload字段ignore list。
- [x] main fixture provenance可重复验证且未修改reference worktree。

### Commit

`test(parity): 固定 main 会话行为判定器`

## 5. W1 — Codex 0.144.1 Owned Protocol 与生成链

### Ownership

- 根Codex dependency pins与lock。
- `agentdash-agent-protocol-codegen`。
- `agentdash-agent-protocol` generated standard types与typed extension composition。
- generated Rust/TypeScript/schema/lock manifest。
- 不修改Runtime、API、connector业务mapper或frontend renderer。

### Work

- 审计并统一全部Codex Rust/npm/schema/fixture pin到`rust-v0.144.1 / 0.144.1`。
- codegen write/check固定schema digest、root allowlist、generator与extension revision。
- vendor↔owned representative/all-root strict JSON roundtrip，覆盖null/omitted。
- 必须手写的窄overlay/block绑定schema path/hash，双向检查missing/extra fields；不得手抄完整标准DTO。
- 保证vendor依赖只进入codegen与Codex integration。
- 证明main已有JSON场景未被版本升级改变；如官方schema强制冲突，停在G1请求决策。

### Exit criteria

- [x] `pnpm contracts:check`通过且fresh generation文件树一致。
- [x] workspace无旧Codex version pin。
- [x] vendor↔owned JSON deep equality覆盖所有纳入root。
- [x] dependency graph中Runtime/Application/frontend无vendor crate。
- [x] main已有fixture在owned`0.144.1`合同下保持protected body一致。

### Commit

`chore(protocol): 对齐 Codex 0.144.1 生成基线`

## 6. W2 — Immutable Presentation Carrier

### Ownership

- `agentdash-agent-runtime-contract` event/wire/IDs/snapshot contracts。
- `agentdash-agent-runtime` reducer所需共享carrier skeleton。
- `agentdash-agent-runtime-wire`。
- 不实现具体connector/tool/frontend行为。

### Work

- 引入`RuntimeJournalFact::Presentation(ImmutablePresentationEvent) | Internal(RuntimeInternalEvent)`等价合同。
- presentation payload使用W1 owned contract完整承载；不再使用窄化`RuntimeItemContent`作为UI事实。
- canonical/source coordinate、operation/binding/revision/cursor只放wrapper metadata。
- presentation payload内部ID绝不canonicalize；source request ID与Runtime interaction ID并存于不同层。
- snapshot transcript从完整terminal payload派生；transient delta携带完整presentation event。
- 删除/禁止从Runtime summary反推presentation的公共接口。

### Exit criteria

- [x] producer payload经过serialize/deserialize/reducer/snapshot roundtrip deep equal。
- [x] Runtime内部坐标变化不改变protected body。
- [x] source IDs、event timestamps、explicit null保持不变。
- [x] contract tests证明internal record不会进入session presentation stream。

### Commit

`refactor(runtime): 引入不可变会话展示载荷`

## 7. W3 — Persistence、Repository 与 Migration

### Ownership

- Runtime memory/repository实现。
- infrastructure PostgreSQL Runtime persistence。
- migrations与migration registry。
- Runtime application facade的commit/read接口。

### Work

- 按W2 schema存储presentation/internal facts并保持事务顺序。
- repository/UoW对调用方提交的任意有序presentation records原子保序；persistence测试使用抽象`A → B`记录，不在本层构造user/turn业务事件。
- durable/transient cursor、replay、idempotency与terminal状态更新不改payload。
- 新增migration清理/重建错误预研journal/snapshot数据；无兼容reader、dual write或fallback。
- memory与PostgreSQL行为一致。

### Exit criteria

- [x] clean database migrate通过。
- [x] 现有开发schema升级/清理路径通过。
- [x] memory/PostgreSQL commit→read→replay protected body deep equal。
- [x] duplicate/gap/recovery/terminal测试不漏发、不重写payload。

### Commit

`refactor(database): 迁移会话展示载荷存储`

## 8. W4 — Codex Connector 完整桥接

### Ownership

- `agentdash-integration-codex`。
- 所有来自Codex vendor stream的标准item/tool event映射与source correlation。
- Codex driver-specific conformance fixtures。
- 不修改Runtime carrier、API projector、tool owner或frontend。

### Work

- 对照main Codex bridge逐method恢复notification/item/delta/status/title/diff/plan/usage/error/compacted。
- vendor typed deserialize后strict transcode完整payload并直接commit presentation event。
- source turn/item/request IDs贯穿start/delta/terminal/response；Runtime ID只在carrier。
- 按main恢复各server request的自动响应或presentation interaction行为，不能统一改成另一种approval。
- admission无静默catch-all；`0.144.1`新纳入family原样承载。

### Exit criteria

- [x] main Codex fixture逐事件deep equal。
- [x] 同一item body/delta/terminal ID一致。
- [x] usage/error/status/title/diff/plan/compaction无压缩或丢失。
- [x] unsupported method显式失败/diagnostic，无文本化fallback。

### Commit

`fix(codex): 恢复 Codex 会话事件无损桥接`

## 9. W5 — Native 与 Remote/Relay Connector 恢复

### Ownership

- `agentdash-integration-native-agent` message/reasoning/provider/usage/error/approval/compaction mapping，以及Native vendor stream的`ToolCallStart/Delta/End`、`ToolExecutionUpdate/End`映射与source correlation。
- Remote Runtime/Relay presentation wire透传。
- 不修改AgentDash ToolBroker/Business Surface contribution emitter；该范围留给W6。

### Work

- 对照main Native stream mapper分支恢复事件数量、顺序、ID和payload。
- User replay只correlate；Assistant MessageStart不生成空ItemStarted。
- AgentMessage与Reasoning独立item和durable terminal；usage顺序与main一致。
- provider全部phase、diagnostic+error、compaction、approval requested/resolved完整。
- Remote/Relay只重包allowlisted wrapper，presentation body原样。

### Exit criteria

- [x] 无phantom tool card源事件。
- [x] reasoning refresh后仍有独立durable terminal。
- [x] Native全场景与main fixturedeep equal。
- [x] Remote serialize/relay/deserialize前后protected bodydeep equal。

### Commit

`fix(native): 恢复 Native 与 Remote 会话事件桥接`

## 10. W6 — 全 Tool Catalog Owner Projection

### Ownership

- AgentDash surface/tool broker projection contract。
- 最终catalog中由AgentDash ToolBroker/Business Surface contribution产生的所有tool owner实现。
- tool-specific fixture与admission tests。
- 不修改`agentdash-integration-codex/**`或`agentdash-integration-native-agent/**`。

### Work

- 动态枚举最终Host/Business Surface Tool Catalog，不使用静态“已覆盖”清单代替。
- 每个contribution声明`vendor_stream`或`tool_broker`唯一presentation emitter，禁止同一生命周期双发；vendor stream emitter分别归W4/W5。
- 每个contribution声明main实际ThreadItem/Platform family、started/update/result/error/approval builders与identity。
- command/shell、file/apply patch、fs read/grep/glob、MCP、dynamic、Workspace/Canvas、Companion、Task/Wait、terminal/control及inventory新增项全部覆盖。
- main使用DynamicToolCall+Platform fact的工具保持该表达；禁止新vfs/workspaceModule/companion/task/wait presentation discriminant。
- ToolProgress必须映射为main对应ItemUpdated/typed delta并进入stream。

### Exit criteria

- [x] catalog contribution数=有projector数=有完整fixture数。
- [x] 缺projector/fixture时surface admission失败。
- [x] 每个tool的call/update/result/error/approval与main deep equal。
- [x] 无按tool name猜测、implicit Dynamic或generic JSON fallback。

### Commit

`fix(tool): 恢复完整 Tool Catalog 协议投影`

## 11. W7 — Application 与 Platform Event Producers

### Ownership

- runtime-session/AgentRun/lifecycle/hook/title/terminal/context/control-plane等application producer。
- Platform event builders与producer tests。
- 不修改journal route或frontend renderer。

### Work

- 以main全部`persist_notification`/event writer inventory为账本恢复producer。
- user submit producer唯一负责按`UserInputSubmitted → TurnStarted`构造并提交完整事件；W3只原子保序，不构造业务事件。
- user/system/workflow/companion delivery、turn terminal/rewind、title、hook trace、session meta、provider/diagnostic、control-plane、terminal/PTY、context compaction、fork marker等逐项恢复。
- 每个producer直接构造完整immutable presentation event并提交W2 carrier。
- AgentRun启动必须从产品delivery session边界提供强类型`PresentationThreadId`；该identity经`ThreadStart → RuntimeThreadState → outbox → DriverCommandEnvelope`原样透传，所有protected payload的`threadId`只取该字段。
- 内部Runtime fact与presentation fact分开提交，禁止API后补。

### Runtime Surface 收口

- `RuntimeSurfaceDescriptor` 与 `SurfaceAdopt` 统一表达 frame、VFS、context、settings、tool 与 hook 版本引用；Managed Runtime 在同一 mutation/CAS/UoW 边界验证并投递 immutable target。
- broker 按 binding/revision/digest 保留历史物化版本，Host 仅在 driver 返回精确 `AppliedSurface` 后 CAS 更新 binding；Codex 在 idle 边界对同一 thread 执行完整 resume 后原子替换 session，Native 暂以显式 unsupported capability 保持悬空。
- `BusinessFrameSurfaceQuery`、`AgentRunRuntimeSurfaceUpdateService` 与 Workspace production bridge 已接入 canonical adopter，避免以 `ToolSetReplace` 代替完整 surface lifecycle。

### Exit criteria

- [x] main production writer inventory 100%有current owner与fixture。
- [x] input source、entry index、turn/tool coordinate与main一致。
- [x] Platform variant事件数量、顺序与payload deep equal。
- [x] 每个AgentRun启动fixture证明`PresentationThreadId`来自原产品delivery session，并在持久化projection、outbox重放与driver dispatch后保持不变；`RuntimeThreadId`和source thread identity仅用于routing/carrier。
- [x] 不存在依靠API filter隐藏缺失producer的路径。

### Commit

`fix(session): 恢复应用会话事件生产者`

## 12. W8 — AgentRun Journal、History、NDJSON 与 API

### Ownership

- AgentRun journal application service。
- session/journal query、history、GET page、NDJSON stream adapter。
- history/fork projection、headers/cursor/heartbeat。
- 删除所有presentation反向重建器。
- W8只拥有fork历史读取，不拥有fork创建命令或其他控制面mutation endpoint。

### Work

- journal API只读取`Presentation`record并重新包装allowlisted metadata。
- 恢复main inherited fork prefix、marker，并将新的wrapper identity/sequence映射为main等价的稳定target/session语义。
- GET/NDJSON共享projection；initial顺序、heartbeat、resume、lagged/closed、keep-alive恢复main行为。
- runtime inspect/internal stream保留独立endpoint，不作为session feed替代。

### Exit criteria

- [x] `runtime_presentation_event`及等价mapper删除。
- [x] GET=initial NDJSON=reconnect=refresh的presentation sequence/body一致。
- [x] fork inherited/marker/entry/round coordinates通过。
- [x] stream order、heartbeat、resume与error handling通过main行为测试。
- [x] API读取同一durable record不改变event timestamp/ID/null。

### Commit

`fix(stream): 恢复会话历史与实时事件流`

## 13. W9 — `features/session` 直接恢复

### Ownership

- `packages/app-web/src/features/session`。
- session-specific generated adapter/validator与tests。
- 不修改AgentRun command/product state。

### Work

- 对main目录执行逐文件no-index diff并直接恢复feed、stream、reducer、renderer、registry、turn segmentation、system dispatcher、round actions。
- 唯一允许差异：W8 envelope unwrap seam、W1 generated type import、官方`0.144.1`nullable类型要求。
- 普通MessageStart不进入tool renderer；真实item lifecycle按main卡片绘制。
- 删除第二renderer、Runtime-aware reducer和generic fallback。

### Exit criteria

- [x] frontend file ledger除显式allowlist外差异为零。
- [x] user/assistant/reasoning/plan/tool/context/usage/error/interaction/round fixtures通过原renderer。
- [x] 用户不会变Agent/CHANNEL；无phantom tool card；reasoning刷新不丢。
- [x] `pnpm --filter app-web test`相关session tests与typecheck通过。

### Commit

`fix(frontend): 恢复 features/session 原会话表现`

## 14. W10 — AgentRun 外层产品行为恢复

### Ownership

- frontend AgentRun services、workspace command/control model、page与相关UI。
- backend composer/cancel、fork/fork-submit、mailbox action/recall/resume、context projection/compact、interaction respond/approval mutation endpoints。
- W10只拥有fork创建命令，fork历史读取归W8。

### Work

- 对照main恢复conversation command authority、ownership、stale guard和availability。
- 恢复submit/cancel/compact、model/backend selection、accepted refs/redirect。
- 恢复fork/fork-submit、round intent、mailbox waiting/actions/recall/resume、context projection/compact。
- 恢复status bar target、`onSystemEvent`、control-plane effects、lineage、parent/children/run detail与页面行为。
- Runtime inspect/capability不得替换生产控制面。

### Exit criteria

- [x] backend route/service ledger与main一致或仅多出不改变main行为的internal Runtime endpoint。
- [x] frontend AgentRun outer file/behavior ledger清零。
- [x] fork/mailbox/context/approval/redirect/status/system side-effect E2E通过。
- [x] 不在前端伪造command set、mailbox waiting/action或stale guard。

### Commit

`fix(agentrun): 恢复 AgentRun 外层交互行为`

## 15. W11 — 全链路 Parity、Spec 与收口

### Work

- 对全部W0 deterministic场景运行main golden与current全链路比较。
- Codex/Native/Remote、全Tool Catalog、application Platform、fork/history/reconnect/refresh全部通过。
- 运行main/current浏览器场景并比较entry类型、顺序、card、round action和side effects。
- 执行dependency、dead code、route ledger、generated drift与migration audit。
- 更新`.trellis/spec/`为最终正确架构，只记录为什么使用immutable presentation payload与wrapper分层。

### Required commands

```powershell
pnpm contracts:check
cargo test -p agentdash-agent-protocol
cargo test -p agentdash-agent-runtime-contract -p agentdash-agent-runtime -p agentdash-agent-runtime-test-support
cargo test -p agentdash-integration-codex -p agentdash-integration-native-agent
cargo test -p agentdash-application-agentrun -p agentdash-api
pnpm --filter app-web run typecheck
pnpm --filter app-web run lint
pnpm --filter app-web test
```

代表性E2E使用`pnpm dev`启动完整链路；Rust变更后先结束旧进程再重新启动。浏览器中文输入遵循项目AGENTS.md的UTF-8脚本约束。

### Exit criteria

- [ ] wrapper normalization后全部eventstream protected body deep equal。
- [ ] route/history/stream/frontend/AgentRun observable behavior与main一致。
- [ ] 无dual-read、fallback、第二renderer、反向presentation mapper或generic fallback。
- [ ] 所有quality gates通过，数据库migration已验证。
- [ ] 每个工作项都有独立implement/check记录与单独提交。

### Commits

- `test(session): 建立 main 全链路行为等价门`
- `docs(spec): 固化会话载荷与运行时边界契约`

## 16. 独立 Check 规则

每个check agent必须：

1. 从`check.jsonl`加载main oracle、对应spec、当前工作项实现与W0 fixture。
2. 不复用implement agent的完成声明或profile fidelity结论。
3. 先审查main/current生产路径，再运行tests。
4. 明确报告每个fixture/owner的覆盖率和未覆盖项。
5. 检查本工作项是否越过ownership，并确认并行修改未被覆盖、格式化、暂存或回退；Cargo锁等待本身不算失败。
6. 失败时给同一implement agent可执行的文件/合同缺口，保持任务继续运行。

## 17. Rollback Points

- G1失败：停在protocol/oracle，不能扩大wrapper allowlist或手抄DTO。
- G2失败：回到W2/W3，不能在API/frontend补默认字段。
- driver/tool/application inventory不完整：不得进入G3/W8。
- W8发现payload缺字段：回到对应producer，不在journal route重建。
- W9/W10发现UI行为差异：先检查W0 fixture与producer；仅main本身使用该前端逻辑时才修改renderer。
- W11任一deep parity失败：任务保持`in_progress`，不得archive或勾选整体完成。
