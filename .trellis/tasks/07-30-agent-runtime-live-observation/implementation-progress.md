# 实施进度

## 已完成：Snapshot / Live Lane 第一纵向切片

- 抽取不含conversation的`AgentObservationState`。
- Complete Agent live合同收束为原子`AgentLiveBatch`。
- Native durable commit一次发布exact suffix与同一committed history折叠出的state。
- ephemeral provider/message/reasoning/tool presentation只发布`state = None`。
- Runtime live转发路径不再调用authoritative read。
- 浏览器`AgentRuntimeUpdate`不再携带完整conversation。
- AgentRuntimeConnection不再把live presentation累积回authoritative view，也不再为正常terminal例行refresh。
- Session stream只在baseline replacement时完整重建；普通update按batch增量归约。
- active turn成为Thinking的权威gate；terminal turn对迟到waiting是吸收态。
- API stream序列化与source stream错误已增加target-scoped诊断，不再无信息静默退出。
- generated Rust/TypeScript schema与codecs已同步。

## 验证证据

- 100个live batch投影期间authoritative read计数为零。
- Native input admission以一个durable batch发布`UserInputSubmitted + TurnStarted + active state`。
- terminal后迟到provider waiting不能重新显示Thinking。
- Runtime/session前端定向测试与TypeScript typecheck通过。

## 已完成：Typed Stream / Reset 第二纵向切片

- 浏览器流收束为generated `Baseline / Update / ResetRequired` frame。
- gateway先建立live订阅再读取baseline，消除read/attach窗口。
- Lagged、sequence gap、source mismatch、protocol error与transport disconnect进入typed reset。
- API为每条连接分配epoch，并记录target、source、epoch、last sequence、lag count/error详情。
- 前端初始与重连baseline统一来自stream；同一epoch只清理一次ephemeral lane。
- Session reset立即回到最后baseline，新epoch baseline再替换authority。
- NDJSON parse failure结束当前transport并进入重连，不再静默丢帧。
- 命令后的显式refresh保留，正常terminal与lane recovery不发起并发`/runtime/view`读取。

## 已完成：本地工具 Progress 第三纵向切片

- Core、Dash、Host callback、Runtime Tool Broker和Native mapper统一使用
  `Started -> Progress(update_index) -> Completed`事件流。
- before-tool deny、校验失败、取消、stream error/EOF和正常结果都收敛为唯一terminal。
- Host callback以有界channel和绝对deadline消费工具过程，超出过程背压预算时返回typed failure。
- Native使用owner projector把同一tool item映射为`ItemStarted / ItemUpdated / ItemCompleted`。
- `fs_apply_patch`开始即暴露完整拟议patch，执行后再发布带实际改动的progress与terminal；
  单mount实际路径统一保留mount前缀。
- result-only production callback入口已删除；所有调用方直接消费typed stream。

## 第三切片验证证据

- Core loop测试覆盖started、连续progress、completed顺序。
- Runtime Broker测试覆盖executor progress在terminal前保序转发。
- 真实VFS apply patch测试覆盖proposed与actual两次更新。
- Native生产服务测试覆盖ephemeral canonical
  `ItemStarted -> ItemUpdated -> ItemCompleted`。

## 已完成：RuntimeWire / Remote 第四纵向切片

- RuntimeWire升级至revision 8，Tool callback response成为显式携带
  `effect_id / item_id / tool`的多帧typed lifecycle。
- Remote proxy与Local endpoint按request frame转发完整
  `Started -> Progress(update_index=1..n) -> Completed`序列。
- 两端校验correlation、started顺序、progress连续性与唯一terminal。
- handler拒绝、EOF、disconnect、deadline、乱序、跳号和重复事件统一收敛为唯一failed terminal，
  不遗留pending callback或in-progress item。
- 相同effect与相同request重放完整settled lifecycle；不同request形成typed duplicate conflict，
  不重复调用工具owner。
- Codex adapter使用同一canonical item lifecycle；该路径不存在独立result-only Host callback seam。

## 已完成：Production Composition 收敛

- Native真实`fs_apply_patch`从开始态暴露proposed patch，并以progress持续更新actual changes。
- 真实Shell链路
  `AppliedVfsRuntimeToolService -> ShellExecRuntimeTool -> PlatformToolBroker -> RuntimePlatformToolHandler`
  通过typed Host stream执行。
- 真实Relay MCP链路
  `ProductionRuntimeMcpToolCatalog -> RelayRuntimeMcpTool -> PlatformToolBroker -> RuntimePlatformToolHandler`
  通过同一typed Host stream执行。
- Shell与MCP即使没有progress，也都立即发布`Started`并以唯一`Completed`结束。
- production tracer覆盖waiting、增量文本、工具lifecycle、terminal、reload以及lag/reset/reconnect边界；
  parity测试调用当前production composition。

## 第四切片验证证据

- RuntimeWire生成器与round-trip测试覆盖revision 8及新的Tool event闭包。
- Remote loopback测试覆盖多帧progress、同effect replay、reentrant callback、deadline、gap、
  duplicate terminal与disconnect。
- Production composition测试覆盖Shell和Relay MCP真实调用链，均验证即时started与唯一terminal。
- `pnpm contracts:check`证明Rust authority、TypeScript codec、schema与manifest同步。
- Runtime Contract/Wire/Host、Native、Remote、Codex与production tool tracer定向测试通过。
- Agent Runtime与Session前端定向测试、TypeScript typecheck及本任务文件ESLint通过。
- `git diff --check`通过，实施计划与Review Gates已全部核对完成。

## Review 收敛

- Frontend connection补齐显式refresh期间的ordered live overlay、durable identity确认、
  generation fence、reset invalidation与重复baseline协议保护。
- Session reducer补齐terminal turn对迟到message/reasoning/tool progress的吸收语义；transport
  parse error补齐target、connection epoch与last sequence。
- Native exact suffix、Codex单source pump/owner state/read竞态、Remote live Wire与有序callback
  在独立review中复核并补齐。
- 最终Rust authority重新生成contract/wire TypeScript、schema与manifest；contracts check、
  前端90个相关用例、TypeScript、ESLint、14个受影响crate的cargo check与任务范围Clippy均通过。
- 详细finding、既有非任务Clippy baseline与命令证据记录在`check-review.md`。
