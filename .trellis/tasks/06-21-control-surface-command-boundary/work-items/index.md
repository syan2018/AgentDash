# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CS01 | Lifecycle create / continue command contract | design | completed | D04 | `POST /lifecycle-runs` 只创建 Ready run；后端显式组合 command 承载一键 create+continue |
| CS02 | Lifecycle start/drain 拆分实现 | implementation | completed | D04 | `POST /lifecycle-runs` 只创建 Ready run，新增 continue/drain command 与后端组合 command |
| CS03 | Hook mailbox NotFound fallback 收口 | implementation | completed | D02, D03 | anchored AgentRun missing mailbox diagnostic/error；unbound trace direct path |
| CS04 | Command availability core resolver | design+implementation | pending | D10 | route policy 与 UI snapshot 共用 core，不重建完整 UI projection |
| CS05 | Extension backend target resolver 统一 | design+implementation | completed | D08 | session-bound action/channel 从 runtime session route 解析 backend；Project-level 非 session invocation 暂不实现 |
| CS06 | Relay command target taxonomy | design | completed | D09 | execution/session/mount/setup 分类 contract 已明确；Terminal 属 mount utility，session MCP 属 session-route-bound |
| CS07 | Extension channel admission parity | implementation | completed | D13 | channel method permission known-key 预检与 action admission 对齐 |
| CS08 | Terminal vs execution lease 产品语义 | design | completed | D18 | Terminal 是 mount utility；completion 通过可恢复 outbox 回调进入 AgentRun steer/turn-boundary |
