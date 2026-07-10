# Decision Ledger

## Resolved

| ID | Decision | Evidence / Reason |
| --- | --- | --- |
| D-C01 | `Channel` 只表示有状态通信空间；Extension request/response provider contract 改名 `ExtensionProtocol`。 | 两者身份、生命周期、participant、delivery 与权限不变量不同。 |
| D-C02 | ExtensionProtocol method 通过 canonical `Operation` 暴露；底层 invoker 只保留 adapter provenance。 | 避免 Workspace Module/Canvas/Agent 各建调用协议。 |
| D-C03 | Channel 使用多 owner 领域模型与 owner-local persistence。 | 07-07 用户决策、归档 PRD/design 与当前 database spec。 |
| D-C04 | LifecycleRun runtime Channel 保留 `channel_registry`；Project Channel 物理承载归 Project Assets。 | runtime fact 随 owner 生灭；Project Channel 是明确产品资产。 |
| D-C05 | 当前 ref 为 `ChannelRef { owner, channel_id }`，稳定业务寻址为 `ChannelLocator { owner, channel_key }`。 | 与 owner-local store routing 同源，不保留两套 authority。 |
| D-C06 | Channel 不保存 Interaction canonical command/event/state，也不复制 Mailbox/Gate/Terminal payload。 | 事务边界和恢复 authority 不同。 |
| D-C07 | 父任务使用 `work-items/` 统一管理落实步骤。 | 用户 2026-07-10 确认。 |

## Evidence-Gated Technical Decisions

- Project、LifecycleRun owner 保留；Story/System 只有在出现独立创建者、lifetime、query/store 和权限不变量时保留。
- independent aggregate 只有跨 owner query、独立 retention/claim、不可重建 binding reverse index、跨 owner audit 或数据库唯一约束需要时才重新评审。
- aliases 只用于显示/搜索，不承担 authority；ChannelKey owner-local unique。
- full message event log 不在本任务引入；bounded delivery state 只服务恢复/去重。

当前没有需要重新向用户确认的 Channel 核心产品决策。若上述 evidence gate 触发独立 aggregate，必须新增 open product decision 并暂停 WI-03。
