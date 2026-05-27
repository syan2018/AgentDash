# Implementation Plan

## Steps

1. 补 relay/application 类型：新增 workspace invocation context，并接到 action/channel relay payload。
2. 在 API route 中解析 session VFS default mount，并写入 action metadata/channel request。
3. 调整 local extension host activation 与 ActiveExtension，保存 `default_workspace_root`。
4. 修改 host API root 解析：默认 root 使用 session workspace root，不再 fallback 到 process-level `workspace_roots`。
5. 恢复 protocol-demo workspace/process 示例与测试。
6. 更新/补充聚焦测试。
7. 运行格式化与聚焦验证。

## Validation

- `cargo test -p agentdash-application runtime_gateway::extension_actions`
- `cargo test -p agentdash-local extensions::host::permissions`
- `pnpm --dir examples/extensions/protocol-demo run test`
- `pnpm --dir examples/extensions/protocol-demo run pack`
- `pnpm --dir examples/extensions/protocol-demo run validate`

不跑全量测试；本任务只验证 extension runtime payload、local host root 解析和示例插件打包/校验。
