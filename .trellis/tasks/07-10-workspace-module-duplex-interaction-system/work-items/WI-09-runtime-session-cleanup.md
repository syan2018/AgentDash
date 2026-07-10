# WI-09 RuntimeSession Cleanup

Status: planned

Depends On: WI-02、WI-05、WI-06、AgentRun adapter migration

## Scope

- Gateway/provider/API/frontend 中 Session-bound Canvas/Extension authority 全量替换。
- 删除 Session consumer variants、required session ids 与 backend placement inference。
- 统一 trace correlation refs。
- 删除 legacy routes/DTO/tests/docs。

## Exit Criteria

- standalone User/Extension/Canvas path 无 RuntimeSession dependency。
- AgentRun path 只通过 AgentFrame adapter 消费同一 Gateway。
- 没有双路径、fallback 或旧 contract alias。

## Validation

- repository-wide RuntimeContext::Session/runtime_session_id usage classification。
- API/contracts/frontend static checks。
- AgentRun + standalone integration tests。
