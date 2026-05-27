# Extension Host 使用 Session Workspace Root

## Goal

Extension runtime action 与 protocol channel 在调用本机 TS Extension Host 时，默认工作目录必须来自当前 session 的 VFS default mount，而不是 local backend 进程级 `workspace_roots`。

这样 `ctx.api.workspace.*`、`ctx.api.process.shell()`、`ctx.api.process.exec()` 在插件内无需显式传 `workspace_root` 就能落到当前会话的工作区目录；`workspace_roots` 仍可作为本机已登记根集合/诊断信息存在，但不再承担 session invocation 的默认 cwd 语义。

## Requirements

- API route 在校验 Project/session/backend 关系后，从 session context plan 的最终 VFS surface 中解析当前 backend 对应的 default workspace mount。
- RuntimeGateway extension action 与 channel invocation payload 要把解析出的 session workspace root 透传给 relay/local runtime。
- Local TS Extension Host activation 要保存 session default workspace root，并用它作为 host workspace/process API 的默认 root。
- 当调用参数显式携带 `workspace_root` 时继续尊重显式 root；未携带时只使用 session default workspace root。
- 如果 session 没有可用 workspace root，workspace/process API 应返回清晰错误；不再静默回落到 process-level `workspace_roots`。
- 恢复 `examples/extensions/protocol-demo` 中原本的 workspace/process runtime action、protocol channel 方法、panel 展示与测试，作为 session workspace root 链路的实际示例。

## Acceptance Criteria

- [x] protocol-demo 中 workspace 写/读/list/stat 与 shell 示例恢复，并在打包后 manifest digest 与 bundle digest 正确更新。
- [x] extension action invocation 的 relay payload 携带 session workspace root。
- [x] extension channel invocation 的 relay payload 携带 session workspace root。
- [x] Local Host 在 `workspace_roots` 为空但 session workspace root 存在时，workspace/process API 可以正常解析默认 root。
- [x] Local Host 在没有 session workspace root 且未显式传 root 时，不会使用 process-level `workspace_roots` 兜底。
- [x] 聚焦测试覆盖 relay payload 与 local host 默认 root 行为；不要求跑全量测试。

## Notes

- 当前任务不处理完整插件权限模型设计；通用 shell 权限仍以 manifest/action/channel permission 声明为准。
- `workspace_roots` 作为本机 profile/已登记根集合仍有价值，但不应与 session 当前工作目录混用。
