# WI-01 ExtensionProtocol Atomic Rename

Status: done

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

Completed evidence:

- Manifest、domain、contracts/generated TS、SDK/toolchain、Workspace Module、RuntimeGateway、relay/local host、frontend bridge 与 example 已原子使用 `protocols/protocol_key/invoke_protocol`。
- Operation dispatch 固定 provider extension key/id、protocol key/version 与 method；Gateway direct resolution 拒绝 provider ambiguity，并校验 contract version requirement。
- `pnpm --filter @agentdash/extension test`：37 tests + TypeScript typecheck 通过。
- `cargo check` 与受影响 Rust test targets `--no-run` 通过。
- `pnpm run contracts:generate && pnpm run contracts:check` 通过。
- `pnpm --filter app-web typecheck` 与 13 个 focused frontend tests 通过。
- production/spec static scan 未发现旧 `protocol_channels/ProtocolChannel/invoke_channel/channel_key/custom_channel/extension.channel.invoke` vocabulary。
