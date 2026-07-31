# 正常 ContextFrame 更新与压缩 Reset 证据

## Normal Surface Flow

1. Native integration把Bound Surface转换为包含完整instructions与tools的`DashSurface`。
2. `materialize_surface_frames(current, previous)`为每个stable instruction各生成一个Frame，
   因此同kind的多个来源没有聚合。
3. capability/tool相关sections按`previous → current`生成一个`CapabilityStateDelta`；
   首次调用`previous=None`时自然得到`empty → current`完整状态。
4. `SurfaceApplied`把完整accepted Surface与这些Frames写入Dash history。
5. canonical projector比较previous/current，只发布stable真实变化；`SystemAppend` delta按发生事实发布。

相关位置：

- `crates/agentdash-integration-native-agent/src/service.rs:1863-1875`
- `crates/agentdash-integration-native-agent/src/accepted_context.rs:17-39`
- `crates/agentdash-integration-native-agent/src/accepted_context.rs:115-219`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs:349-390`

## Current Compaction Flow

1. `DashAgentStore::complete_compaction()`生成一个summary Frame。
2. `CompactionApplied`只保存该summary Frame与`retained_from`。
3. context materializer寻找latest completed compaction来确定conversation boundary。
4. system Frames仍由current stable Frame、Initial Context和全历史append ledger拼出。
5. compaction projector调用同一全历史拼装函数，把旧Frames再次发布。

相关位置：

- `crates/agentdash-agent/src/dash/store.rs:143-231`
- `crates/agentdash-agent/src/dash/history.rs:1101-1140`
- `crates/agentdash-agent/src/dash/service.rs:3191-3277`
- `crates/agentdash-agent/src/dash/service.rs:3582-3746`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs:195-212`

## Protocol Has Enough Core Facts, But Retains A Deleted Planner

可以直接复用：

- `ContextDeliveryStatus::AppliedToCompactedContext`表达compaction rebuild；
- `ContextFrameKind::CapabilityStateDelta`与typed sections表达`empty → current`或真实delta；
- `RuntimeContextFragmentEntry`已经保存stable来源所需字段；
- `AgentContextRecipe`已经分别承载Frames、structured tools与messages。

需要删除：

- 无生产消费者的`ContextDeliveryPlan/Target/Entry`；
- 不驱动当前provider的cache policy、model channel、connector profile；
- 被history fold借用的`ContextAgentConsumptionMode::SystemAppend`；
- 只用于旧前端fixture/debug的phase、apply mode、message role与delivery channel；
- 没有当前生产builder的stable text section变体。

因此不新增Context Generation、conversation Frame或第二个context store；用History payload表达
occurrence，用Frame kind表达stable/delta分类，并收缩旧RuntimeSession协议残留。

## Tests That Currently Encode The Old Behavior

- `crates/agentdash-agent/tests/dash_service.rs`中已有断言要求较早的SystemAppend Frame继续出现在未来
  provider prompt；应改成同时覆盖“正常更新前保留、成功压缩后淘汰”。
- `crates/agentdash-integration-native-agent/tests/complete_agent_service.rs`已有首次完整capability、
  subsequent delta与stable Frame change测试，可扩展为reset前后纵向场景。
- 同文件当前明确期待两个Identity Frame，应改成一个Identity Frame包含两个有序fragments，
  并覆盖Guidelines/Environment/UserContext等presentation不再伪装成Assignment section。
- compaction provider capture测试目前只证明provider等于现有recipe，尚未证明recipe不含压缩前
  delta；应加入Frame ID、sections、message boundary和usage断言。

## Migration Constraint

`dash_complete_source.repository`保存整个Dash source document，旧
`CompactionApplied.summary_frame`无法仅靠SQL字段改名生成准确的full current Frame batch。
项目处于预研期，因此迁移应一次性清理不兼容Dash source authority，并同步清除指向这些source的
Product runtime binding及其投递证据，使后续访问走正常provisioning；运行时代码只读取新格式。
