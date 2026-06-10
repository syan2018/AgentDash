# Design: Extension Manifest 单一事实源

## Decision

Extension manifest 是运行 surface 的单一事实源。TS extension entry 只注册 handler 实现；`runtime_actions`、`protocol_channels`、权限、schema 与安装态 projection 以 manifest 声明为准。

原因：

- Shared Library / Project installation / package artifact 都保存 manifest snapshot，后端和本机 runner 已经围绕该事实做投影与审计。
- TS 注册代码是可执行实现，不适合作为安装摘要、权限诊断和跨语言 parser 的权威 schema。
- 预研阶段不保留旧格式兼容，缺失或不一致应 fail-fast。

## Contract

- `runtime_actions[].input_schema` / `output_schema` 和 `protocol_channels[].methods[].input_schema` / `output_schema` 为必填字段，只接受 JSON Schema object 或 boolean，不接受 `null`。
- `runtime_actions[].permissions` 与 channel method `permissions` 仍可缺省，语义为无额外 host API 权限。
- TS SDK 的 manifest / registration 类型与上述字段保持一致。
- JS validator 和 Rust domain parse 对 manifest 字段形状一致：数组字段如果出现必须是数组，schema 字段必须存在且非 null。

## Runtime Parity

- `extension-dev` 在加载 extension 后比较 TS 注册项与 manifest 声明：
  - TS 注册 action 多于 manifest：错误。
  - manifest 声明 action 但 TS 未注册 handler：错误。
  - channel 与 method 同样逐项比较，channel key 使用 canonical provider key。
- `packProject` 在 bundle 后导入 `dist/extension.js`，激活一次 registration context，并在写入 bundle digest 前执行同一 parity 校验。
- 安装态 local runner 在 activate 后按 manifest surface 过滤 handler map，health / invoke 只暴露和调用 manifest 已声明项；不一致直接让 activation 失败。

## Validation

- JS targeted tests 覆盖 validator null/缺失 schema 拒绝、dev runtime parity、pack parity。
- Rust domain tests 覆盖 schema 缺失或 null 被拒绝。
- Local runner tests 覆盖 TS handler 不能绕过 manifest 声明。
