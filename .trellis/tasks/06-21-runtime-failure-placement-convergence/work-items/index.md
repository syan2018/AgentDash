# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| RF01 | backend disconnect projection characterization | research+test | completed | D16 | 已输出 `research/01-failure-placement-characterization.md`，验证 disconnect 后 feed / AgentRun / runtime-summary 是否出现 lost/terminal |
| RF02 | backend disconnect terminal/lost projection | implementation | ready | D16 | 对 running execution 产生 `turn_lost` / `lost` projection；disconnect cleanup 先写 terminal fact 再清理 route/lease |
| RF03 | MCP session context fallback characterization | research+test | completed | D17 | 已输出 `research/01-failure-placement-characterization.md`，验证 session context 下 MCP fallback 到 VFS/catalog/any backend 的当前行为 |
| RF04 | MCP backend fallback 收口 | implementation | ready | D17 | session context MCP list/call route 缺失或 backend 离线直接失败；setup/probe 保留 fallback |
| RF05 | standalone local backend id 来源收口 | design+implementation | completed | Runtime Failure / Placement | standalone CLI 缺少 `--backend-id` 时失败；正式 backend id 只消费 server ensure/claim 或显式 token-bound input |
