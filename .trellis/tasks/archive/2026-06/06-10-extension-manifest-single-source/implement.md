# Implementation Plan

1. 收紧 manifest schema contract
   - JS validator：数组字段出现时必须为数组；schema 字段必填且只接受 object / boolean。
   - SDK 类型：runtime action 与 channel method schema 字段改为必填。
   - Rust domain：移除 schema 默认值，反序列化阶段缺失即失败，校验阶段拒绝 null。

2. 建立 TS registration 与 manifest parity
   - 在 `extension-dev/src/manifest.js` 中提供可复用 parity 校验函数。
   - `dev-runtime` 加载后调用 parity 校验。
   - `packProject` bundle 后导入 extension、收集 registration、调用 parity 校验。

3. 收紧安装态 runner
   - runner activate 后校验并过滤 action/channel handler map。
   - invoke action/channel 只查 manifest 声明后的 handler map。

4. 最小验证
   - `pnpm --filter @agentdash/extension-dev test`
   - `cargo test -p agentdash-domain shared_library::value_objects`
   - `cargo test -p agentdash-local extensions::host`
