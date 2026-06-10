# 架构：extension manifest 单一事实源

## Goal

统一 extension manifest、SDK 类型、dev validator、Rust domain parse 与 runtime projection 的事实源。

## Requirements

- extension manifest 的结构和校验规则只能有一个权威来源。
- `extension-dev` validate/pack、`extension-sdk` 类型、Rust domain parse、安装态 runtime projection、开发态 runtime preview 必须对 `runtime_actions` 和 `protocol_channels` 得出一致结果。
- 开发态 TS 注册项不能绕过 manifest 声明；manifest 声明也不能暴露宿主未实现能力。
- 启动实现前必须决定事实源方向：由 TS 定义生成 manifest，或由 manifest schema 生成 TS/Rust/JS 校验。
- 至少需要 golden fixture/parity 测试覆盖 JS validate 与 Rust domain parse 的一致性。

## Acceptance Criteria

- [ ] JS validator、SDK 类型、Rust domain 对 required 字段和 schema nullability 的规则一致。
- [ ] dev-runtime/pack 阶段能检测 TS 注册项与 manifest 声明的缺失或多余。
- [ ] 安装态 runner 只把 handler 绑定到已声明且已校验的 manifest key。
- [ ] parity 测试能防止 validate 通过但 Rust 安装失败的 manifest。

## Notes

- 这是复杂跨语言任务，当前只作为 tracking task；不要在补齐设计前 start。
