# Review 结果

## Findings (fixed)

- File: `packages/app-web/src/features/agent-run-runtime/model/agentRuntimeConnection.ts`
- Issue: 显式refresh返回的较旧baseline会覆盖读取期间已经交付的live batch；reset或新epoch也没有完整fence该overlay。
- Fix: 增加refresh attempt overlay、stream baseline generation fence与reset invalidation；baseline replacement后按durable presentation identity和state revision重放未确认事实，ephemeral始终保留为live overlay。重复同epoch baseline改为typed protocol reset。

- File: `packages/app-web/src/features/agent-run-runtime/model/agentRuntimeConnection.ts`
- Issue: transport解析失败缺少target、connection epoch与最后成功lane sequence，浏览器侧无法定位协议边界。
- Fix: connection统一包装transport error并附加三项坐标，保留原始error为cause。

- File: `packages/app-web/src/features/session/model/sessionStreamReducer.ts`
- Issue: durable terminal后迟到的同turn message、reasoning与tool progress仍可修改投影并重新打开streaming。
- Fix: reducer维护authoritative terminal turn集合；terminal关闭同turn streaming，后续同turn ephemeral progress只推进lane cursor，新turn继续正常归约。

- File: `crates/agentdash-agent/src/dash/history.rs`, `crates/agentdash-agent/src/dash/store.rs`, `crates/agentdash-agent/src/dash/service.rs`, `crates/agentdash-integration-native-agent/src/service.rs`
- Issue: Native durable callback重复扫描history，并与ephemeral downstream重复发布tool started/completed；sequence分配与broadcast发送之间存在并发反序窗口。
- Fix: commit直接携带exact suffix的逐项fold结果；callback只投影本次suffix与final state；durable wrapper截断started/completed downstream，progress保持ephemeral；sequence分配与发送进入同一临界区。

- File: `crates/agentdash-integration-codex/src/complete_agent.rs`, `crates/agentdash-integration-codex/src/process_transport.rs`
- Issue: Codex未提供可用live lane；全局raw observation sequence会在source过滤后形成假gap，多subscriber会重复fold，terminal不携带idle owner state，`thread/read`还可能覆盖RPC在途期间的新live state。
- Fix: 每source建立单pump并broadcast；raw sequence与连续source batch sequence解耦；observation按raw sequence幂等fold；turn start/terminal、thread name与interaction直接更新owner state；read以fold boundary保留RPC期间的较新事实；retention gap改为source-scoped，disconnect唤醒waiter。

- File: `crates/agentdash-agent-runtime-wire/src/complete_agent.rs`, `crates/agentdash-integration-remote-runtime/src/complete_agent.rs`
- Issue: RuntimeWire/Remote没有Complete Agent live订阅与typed terminal error；多帧Tool callback在pre-start错误和并发发送时可能形成非法生命周期或帧反序。
- Fix: 增加`SubscribeLive`与`Batch/Lagged/Protocol/Unavailable`notification；proxy先建receiver再订阅；endpoint无损转发真实batch；出站帧使用有序屏障且保留reentrant callback；pre-start错误规范化为唯一`Started -> failed Completed`。

- File: `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- Issue: 新增Runtime stream诊断使用项目禁止的裸`tracing::warn!`。
- Fix: 改用`diag!(Warn, Subsystem::AgentRun, ...)`并保留target、source、epoch、sequence/error字段。

- File: `packages/app-web/src/generated/agent-runtime-*.ts`, `schemas/agent-runtime-*.json`
- Issue: 最终Rust authority变化后生成物尚未统一刷新。
- Fix: 从最终contract/wire generator重建TypeScript、schema与manifest，并通过全量`pnpm contracts:check`。

- File: `.trellis/spec/backend/agent-runtime-{native,codex}-adapter.md`, `.trellis/spec/cross-layer/{backbone-protocol,agent-runtime-wire-relay}.md`, `.trellis/spec/frontend/{architecture,hook-guidelines}.md`
- Issue: specs仍缺少batch、stream frame、subscribe-first、refresh overlay、terminal absorption与Remote/Codex live合同。
- Fix: 按最终实现同步不变量、错误语义、竞态原因与required tests；`check.jsonl`补充Codex adapter审查触点。

## Findings (not fixed)

- 未改文件存在既有strict Clippy warning：`agentdash-agent-protocol`和`agentdash-agent-runtime-contract`的`large_enum_variant`、VFS shell test的`await_holding_lock`、Codex canonical projector的`items_after_test_module`、Infrastructure provisioning的`obfuscated_if_else`、API其他route的`manual_clamp/manual_map/needless_borrow`。
- 原因：这些文件不属于本任务diff，项目明确禁止碰工作区既有修改。排除这些已确认基线类别后，本任务14个受影响crate以`-D warnings`通过。
- Blocker: 无。

## Verification

- Lint: pass — 本任务前端ESLint通过；14个受影响Rust crate在排除上述未改基线warning类别后以`cargo clippy --no-deps --all-targets -D warnings`通过；定向rustfmt通过。
- TypeCheck: pass — `pnpm --dir packages/app-web typecheck`通过；14个受影响Rust crate的`cargo check --all-targets`通过。
- Tests: pass — 前端16个文件/90个用例通过；Agent、RuntimeWire、Codex、Native、Remote五包完整tests与doctests通过；Remote最终排序测试、Codex source-scoped gap测试通过。
- Contracts: pass — `pnpm contracts:check`通过，contract/wire TypeScript、codec、schema与manifest一致。
- Diff: pass — `git diff --check`通过。
