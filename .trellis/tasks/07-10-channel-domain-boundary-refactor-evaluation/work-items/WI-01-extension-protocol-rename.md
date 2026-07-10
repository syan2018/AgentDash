# WI-01 ExtensionProtocol Atomic Rename

Status: planned

Depends On: WI-00；Workspace task canonical Operation contract

## Scope

- manifest/domain/contracts/generated TS/SDK/toolchain/Workspace Module/Gateway/relay/local host/examples/docs 全链改名。
- provider-qualified protocol ref 与 contract version resolution。
- artifact/install snapshot migration/rebuild。

## Exit Criteria

- 非通信领域不再使用 channel/channel_key/invoke_channel 命名。
- global first-match resolution 删除。
- Operation descriptor 保存 exact provider/protocol/method/version provenance。
- 无兼容字段、双读或旧 host method。

## Validation

- Extension validate/pack/typecheck/host parity。
- contracts check、Workspace Module/Gateway tests。
- repository-wide old vocabulary scan。
