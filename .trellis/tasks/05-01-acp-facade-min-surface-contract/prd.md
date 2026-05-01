# facade: acp 最小嵌入面契约

## Goal

定义 ACP 作为外部嵌入面的最小方法与事件投影契约，确保接入成本低、内部语义不漂移。

## Requirements

* 固化最小方法面：`session/new`、`session/prompt`、`session/update`、`session/cancel`、history/read。
* 定义 Backbone 到 ACP 的投影规则与字段约束。
* 定义 `_meta` 扩展位的使用边界与保留空间。
* 给出嵌入场景的最小集成示例。

## Acceptance Criteria

* [ ] 形成最小方法面契约文档。
* [ ] 形成 Backbone→ACP 事件映射规则与样例。
* [ ] 形成 `_meta` 扩展约束（哪些可透传，哪些需规范化）。
* [ ] 形成外部嵌入接入说明（v0）。

## Out of Scope

* 不让 ACP 承担内部运行时事实语义。
* 不扩展到完整控制面（thread/turn 管理全暴露）。
