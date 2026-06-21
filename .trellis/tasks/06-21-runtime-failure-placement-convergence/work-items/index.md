# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| RF01 | backend disconnect projection characterization | research+test | completed | D16 | 已输出 `research/01-failure-placement-characterization.md`，验证 disconnect 后 feed / AgentRun / runtime-summary 是否出现 lost/terminal |
| RF02 | backend disconnect terminal/lost projection | implementation | blocked_by_RF01 | D16 | 对 running execution 产生用户可见 terminal/lost projection |
| RF03 | MCP session context fallback characterization | research+test | completed | D17 | 已输出 `research/01-failure-placement-characterization.md`，验证 session context 下 MCP fallback 到 VFS/catalog/any backend 的当前行为 |
| RF04 | MCP backend fallback 收口 | implementation | blocked_by_RF03 | D17 | session context 强制 session route/backend execution，setup/probe 保留 fallback |
| RF05 | standalone local backend id 来源收口 | design+implementation | completed | Runtime Failure / Placement | standalone CLI 缺少 `--backend-id` 时失败；正式 backend id 只消费 server ensure/claim 或显式 token-bound input |
