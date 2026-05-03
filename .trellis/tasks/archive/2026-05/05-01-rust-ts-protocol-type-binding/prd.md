# Rust→TS 协议类型直出

## Goal

通过 rs-ts 将 backbone 事件类型从 Rust 单一来源直出到前端 TypeScript，前端直接消费原始事件类型。

## 背景

当前前端通过手写的 ACP TypeScript 类型消费会话事件。切换到 backbone 事件模型后，前端需要消费的类型来源变为 Rust 侧定义的 `BackboneEvent` 及其子类型。

Codex 协议自身已经带有 `ts_rs::TS` derive（可通过 `codex-app-server-protocol` 的 `generate_ts` 生成 TS 类型），这是一个可以直接利用的优势。

## 方案

### 类型导出清单

1. `BackboneEvent` 枚举 → 前端消费的顶层事件类型
2. `BackboneEnvelope<T>` → envelope 包裹类型
3. `PlatformEvent` → 平台自有事件
4. Codex `ServerNotification` 子类型 → 通过 re-export 或 ts-rs 直接导出

### 生成流程

1. Rust 侧在 `BackboneEvent` 及子类型上 derive `ts_rs::TS`
2. 构建时通过 `cargo test` 或专用 binary 生成 `.ts` 文件到指定目录
3. 前端通过路径引用消费，禁止手写镜像结构

### 前端消费规范

- 直接消费原始事件类型进行渲染，不引入 Render DTO 中间层
- 事件类型变更时重新生成并检查 diff
- 如果 Codex 上游类型变更，重新生成即可检测到影响面

## Acceptance Criteria

* [ ] BackboneEvent 相关类型可通过 rs-ts 生成 TS 定义
* [ ] 前端可 import 生成的类型并类型检查通过
* [ ] 存在生成脚本和产物目录约定
* [ ] 类型变更检查流程文档化

## Dependencies

* 依赖 `backbone-event-model` 任务的类型定义

## Out of Scope

* 不引入 Render DTO 层
* 不处理 UI 组件渲染逻辑
