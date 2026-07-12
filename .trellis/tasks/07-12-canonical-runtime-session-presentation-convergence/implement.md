# Canonical Runtime 会话展示链路执行计划

## 1. 工作项组织原则

只使用当前一个Trellis任务，不创建child task。实现按四个有序工作项推进：

1. **W1 Owned Conversation Protocol**：上游schema导入/codegen、完整同构协议、Codex strict transcode与conformance fixtures。
2. **W2 Runtime Envelope Projection**：journal/snapshot/events携带完整conversation payload，完成schema/migration/API stream。
3. **W3 Connector / Tool Protocol Projection**：逐driver和逐Tool Catalog contribution恢复typed bridge，建立projection descriptor与conformance matrix。
4. **W4 Session Presentation Restoration**：直接恢复`af21f9d7c^`组件图并只替换envelope adapter，删除`AgentRuntimeFeed`。

依赖顺序为W1 → W2 → W3 → W4。每个工作项使用独立、边界明确的implement subagent派发；完成后由独立check subagent按该工作项exit criteria复核，主会话审阅diff和跨项contract后才进入下一项。任务全程保持同一个active task，最终统一执行全链路检查、spec更新、commit与archive。

禁止一次派发覆盖多个工作项；禁止后续工作项以临时fallback绕过前项缺口。进度直接记录在本文件checklist和任务journal，不借child status表达。

## 2. W1 — Owned Conversation Protocol

- 先将workspace内Codex Rust crates、npm依赖、protocol revision检查、schema与fixture统一升级到`rust-v0.144.1 / 0.144.1`，清除旧`0.140.0`基线。
- 先做codegen feasibility gate：使用上游`generate_json/generate_ts`输出固定v2 schema，验证目标schema能稳定生成serde Rust types并通过代表性roundtrip。
- 新增workspace内protocol-codegen工具，引入pinned`typify 0.7.0` builder、schema canonicalization、SHA-256 lock manifest、rustfmt与文件树diff。
- 建立protocol generator的write/check模式并提交pinned schema/TS fixtures、lock manifest与generated Rust/TypeScript；标准协议不得手抄。
- 增加根质量门入口与CI check，验证clean checkout无需全局CLI即可重建。
- 收束`agentdash-agent-protocol`依赖，使普通owned协议编译不依赖Codex vendor runtime graph，vendor只进入codegen工具与Codex integration。
- 从schema生成原frontend所需ThreadItem、event、delta、usage、error、interaction标准类型；AgentDash extension手写组合。
- 固定Codex protocol revision和schema/fixture baseline。
- 在`agentdash-integration-codex`改为vendor typed deserialize + strict serde transcode + exhaustive method admission；删除`Value`字段猜测与文本fallback。
- 生成TypeScript并执行schema drift check。
- 验证：协议crate tests、Codex integration tests、JSON equivalence fixtures、contract generation check。
- 文档化Codex revision升级流程、root allowlist变更规则和codegen失败返回设计评审的条件。

### W1 Exit Criteria

- [x] codegen write/check可重复且无需全局CLI。
- [x] Workspace全部Codex version pins与protocol fixtures统一为`0.144.1`。
- [x] 标准协议generated Rust/TS与pinned schema一致。
- [x] Codex standard payload strict transcode与conformance通过。
- [x] Runtime/Application production依赖图未引入Codex vendor crate。

## 3. W2 — Runtime Envelope Projection

- `agentdash-agent-runtime-contract`引用owned conversation contract。
- 重塑Runtime item/event/snapshot，使完整payload与canonical lifecycle共存且只有一个journal事实。
- 补齐typed delta、interaction request、usage与error事件；保持availability、operation、binding、context、hook/recovery语义。
- 建立真实live transient stream合同：generation/sequence、active-turn有界buffer、reconnect去重、target隔离与final durable item收敛；不得用有限durable轮询模拟token stream。
- 更新Managed Runtime reducer、repository、API facade与NDJSON stream。
- 根据schema变化新增数据库migration或明确无需DDL；不保留旧payload兼容reader。
- 验证：journal/replay/snapshot、durable duplicate/gap、transient replay/去重、target、terminal、interaction与真实PostgreSQL相关测试。

### W2 Exit Criteria

- [x] Runtime journal/snapshot/events保存完整typed conversation payload。
- [x] durable cursor与live transient generation/sequence合同通过重连和去重测试。
- [x] schema/migration处理完成，无旧payload兼容reader或dual write。

## 4. W3 — Connector / Tool Protocol Projection

- 建立driver projection profile与共享conformance harness，覆盖Codex、Native、Remote Runtime。
- Codex adapter接通全item/event/interaction strict transcode，删除unknown item文本fallback。
- Native adapter按旧stream mapper行为清单恢复message/reasoning/provider/tool/usage/error/compaction/approval投影。
- Remote Runtime验证Runtime Wire完整payload、delta identity和terminal原样穿透。
- 为`ToolContribution`增加owner-declared projector；surface compile对最终Tool Catalog做全量projector admission。
- 逐项接通command/shell、file change/apply patch、fs read/grep/glob、MCP、dynamic、Workspace Module/Canvas、Companion、Task/Wait及inventory发现的其它工具。
- 为每个family建立call/update/result/error/approval golden tests；无generic fallback。

### W3 Exit Criteria

- [x] 最终driver inventory全部通过projection conformance。
- [x] 最终Tool Catalog每个contribution都有owner-declared projector。
- [x] command/file/fs/MCP/dynamic/workspace/companion/task代表性E2E保留typed语义。
- [x] 不存在unknown item文本化、按tool name猜测或implicit DynamicToolCall fallback。

## 5. W4 — Session Presentation Restoration

- 从`af21f9d7c^`直接恢复原消息presentation组件图、model、transport和测试文件。
- 手工合并当前`SessionChatView`的canonical command/interaction/product projection修复，禁止整文件回退覆盖后续正确修改。
- 新envelope adapter无损输出原reducer需要的typed conversation events。
- 接回tool cards、diff、MCP、Companion、reasoning、plan、context、usage、error、round/fork与terminal projection。
- 删除`AgentRuntimeFeed/useAgentRuntimeFeed`和平行测试/model。
- 验证：session model/UI tests、frontend typecheck/lint、generated contract check、AgentRun workspace E2E。

### W4 Exit Criteria

- [ ] 原session presentation组件图恢复为生产入口。
- [ ] `AgentRuntimeFeed/useAgentRuntimeFeed`与平行view model删除。
- [ ] `af21f9d7c^`关键UI行为清单逐项通过新envelope驱动测试。
- [ ] 当前canonical command availability、interaction与后续产品projection修复未被回滚。

## 6. 主任务最终审查

- 检查依赖方向：Codex vendor crate只在Codex integration；Runtime/Application/frontend不直接依赖vendor DTO。
- 检查数据流：单一Runtime journal/snapshot/event stream，无Backbone authoritative runtime、dual-read或fallback。
- 比较`af21f9d7c^`与最终session presentation能力清单，逐项证明行为保持。
- 搜索并拒绝`JSON.stringify`工具展示、unknown item转AgentMessage、generic text fallback与第二套feed renderer。
- 枚举最终driver inventory和Tool Catalog，确认每个producer都有explicit projection与conformance evidence。
- 运行相关quality gates和代表性E2E，记录任何数据库migration要求。

## 7. Rollback Points

- W1未证明schema codegen可重复、标准payload roundtrip与全family conformance前不得进入W2；若生成工具不能保真，回到设计评审，不允许改成人工镜像。
- W2未证明snapshot+events完整保真前不得进入W3/W4。
- W3任一driver/tool缺少typed projector时不得进入W4；回到tool owner或adapter补齐。
- W4出现UI parity缺口时回到W1-W3的projection/contract补字段，不允许在renderer中猜测或新建fallback。

## 8. 启动前检查

- [ ] 用户审阅并批准`prd.md`、`design.md`、`implement.md`。
- [ ] W1-W4的共享`implement.jsonl`与`check.jsonl`包含真实spec、基线代码和相关研究；每次派发只选取当前工作项相关子集。
- [ ] 启动当前主任务并从W1单独派发；W1 exit gate前不派发W2。
