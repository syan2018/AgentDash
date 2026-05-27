# Design

## Call Flow

```text
Webview bridge
  -> API route
  -> RuntimeGateway.invoke
  -> ExtensionRuntimeActionProvider
  -> BackendRegistry command
  -> agentdash-local
  -> TS Extension Host
```

## Provider Checks

- action key belongs to enabled Project extension installation
- context is Session runtime
- actor/session matches context/session
- owning backend is online
- action permission is granted by extension installation policy

## Relay Messages

Add command/response pair:

- `CommandExtensionActionInvoke`
- `ResponseExtensionActionInvoke`

Payload includes action key, extension id, project/session ids, input, policy, trace id.
