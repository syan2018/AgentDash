# binding: rust-ts 协议类型直出

## Goal

建立 Rust→TS 的协议类型单一来源机制，使前端直接消费主干原始事件类型，避免手写镜像漂移。

## Requirements

* 明确 Rust 侧协议类型组织和导出清单。
* 配置 rs-ts 生成流程与目标产物目录。
* 约束前端消费方式：优先原始事件直出，不新增 DTO 层。
* 定义类型变更时的生成与校验流程。

## Acceptance Criteria

* [ ] 形成 Rust 类型清单与导出规则。
* [ ] 形成 rs-ts 生成入口与产物约定。
* [ ] 形成前端消费规范（原始事件直出）。
* [ ] 形成类型变更检查清单（防止 drift）。

## Out of Scope

* 不引入 Render DTO 新层。
* 不处理业务组件展示细节。
