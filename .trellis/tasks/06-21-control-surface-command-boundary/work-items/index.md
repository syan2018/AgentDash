# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CS01 | Lifecycle create / continue command contract | design | pending | D04 | create Ready run 与 explicit drain/continue 的 API 语义 |
| CS02 | Lifecycle start/drain 拆分实现 | implementation | blocked_by_CS01 | D04 | `POST /lifecycle-runs` 只创建 Ready run，新增 continue/drain command |
| CS03 | Hook mailbox NotFound fallback 收口 | implementation | ready | D02, D03 | anchored AgentRun missing mailbox diagnostic/error；unbound trace direct path |
| CS04 | Command availability core resolver | design+implementation | pending | D10 | route policy 与 UI snapshot 共用 core，不重建完整 UI projection |
| CS05 | Extension backend target resolver 统一 | design+implementation | pending | D08 | panel API、workspace module tool、RuntimeGateway 使用 server-side resolver |
| CS06 | Relay command target taxonomy | design | pending | D09 | execution/session/mount/setup 分类 contract |
| CS07 | Extension channel admission parity | implementation | ready | D13 | channel method permission known-key 预检与 action admission 对齐 |
| CS08 | Terminal vs execution lease 产品语义 | design | pending | D18 | terminal 是 mount utility 还是 execution surface |

