# 架构：legacy 本机身份链路删除

## Goal

删除 legacy_machine_ids 等兼容身份链路，并同步迁移、contracts、前后端。

## Requirements

- 删除 `legacy_machine_ids` 及相关 legacy identity fallback 语义。
- 同步处理数据库迁移、domain entity、application input、local machine identity、Tauri 入口、contracts、前端 generated DTO 和视图。
- 项目未上线，不需要为旧字段保留兼容读取或回退路径。
- 启动实现前必须设计 migration 方向，明确是修改现有初始化迁移还是新增清理迁移。
- 删除后本机 runtime 身份模型应只保留当前正确字段。

## Acceptance Criteria

- [ ] 代码和 generated contracts 中不再暴露 `legacy_machine_ids`。
- [ ] DB schema 与 readiness/migration 逻辑不再包含 legacy identity 字段。
- [ ] local/Tauri/frontend 调用链使用当前身份字段，且无兼容 fallback。
- [ ] 相关 Rust check、contract generation/check、前端 typecheck 通过。

## Notes

- 这是复杂跨层任务，当前只作为 tracking task；不要在补齐设计前 start。
