# Design

## Process Model

首版由 `agentdash-local` 管理一个 Node-based extension host 进程。后续可演进为 per-extension worker isolation。

## Protocol

JSON-RPC over stdio 或本机 WebSocket：

```text
local -> host: initialize, activate, deactivate, reload, invoke_action, health
host -> local: register_contributions, log, request_host_api, emit_event
```

## Host API Facade

`api.local.getProfile()` 由 local runtime 执行并裁决权限：

- permission: `local.profile.read`
- returns: username/display name, platform, arch, backend id, redacted workspace root, session/project summary

插件代码不直接读取 env/home/token/ssh config。
