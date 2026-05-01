# mapping: codex 到 backbone 语义映射 v0

## Goal

完成 Codex 协议到 Runtime Backbone 的 v0 映射规则，保证 thread/turn/item 主语义一致且可回放。

## Requirements

* 建立 Codex 事件/方法到 Backbone 实体的映射表。
* 明确“默认透传 + 按需封装”的判定规则。
* 输出典型交互链路映射示例（创建、推进、终止、错误）。
* 定义不可映射/冲突场景处理策略。

## Acceptance Criteria

* [ ] 映射表覆盖 thread/turn/item 核心路径。
* [ ] 映射文档包含字段级别规则与示例。
* [ ] 冲突策略可用于实现层直接参考。
* [ ] 与父任务的 Envelope v0 规则一致。

## Out of Scope

* 不定义 ACP facade 的外部嵌入方法面。
* 不落地 conformance 自动化实现。
