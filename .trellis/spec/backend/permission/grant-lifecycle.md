# Permission Grant Lifecycle

Grant以request -> approved/rejected -> applied/revoked/expired演进，并绑定AgentRun、capability/tool/VFS scope与`source_runtime_operation_id`审计证据。

- 同一request id幂等；scope escalation创建新request。
- apply/revoke更新Business Surface revision，不能直接修改Driver live state。
- Tool Broker在每次call验证当前grant revision；过期/revoked在credential解析和tool side effect前拒绝。
- migration 0065使用canonical Runtime operation FK。

测试覆盖用户批准、自动批准、拒绝、过期、撤销、stale surface与operation audit。
