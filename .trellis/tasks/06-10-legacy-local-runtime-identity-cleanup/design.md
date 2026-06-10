# Design: 删除 legacy 本机身份链路

## Migration 方向

本任务授权范围包含 `crates/agentdash-infrastructure/migrations/**`，目标是让预研期初始化 schema 直接表达当前正确模型。因此本次修改 `0001_init.sql`，从 `backends` 表移除 `legacy_machine_ids`，并使用 `ALLOW_MIGRATION_BASELINE_REWRITE=1` 运行 migration guard。替换基线后，已有 embedded PostgreSQL data 目录需要重建。

## 当前身份模型

本机 runtime 身份只使用：

- `machine_id`：由 `agentdash-local` 生成并持久化的稳定机器身份。
- `machine_label`：展示用标签，不参与唯一匹配。
- `profile_id`：桌面端 profile 标识。
- `share_scope_kind` / `share_scope_id` / `capability_slot`：与 `machine_id` 一起定位 local backend。

server ensure API 使用 `machine_id + share_scope_kind + share_scope_id + capability_slot` 定位已有 backend。`device_id` 保留为普通历史字段但不参与本机身份兼容匹配，`machine_label` 只用于展示和默认名称。

## 跨层改动

- Domain/Application：删除 `BackendConfig`、`LocalBackendClaim`、`EnsureLocalRuntimeInput` 的 `legacy_machine_ids`。
- Infrastructure：删除 backends 表列、repository SQL 读写、legacy identity candidates、legacy duplicate merge 测试。
- API/Contracts：删除 ensure request 输入和 backend response 中的 legacy 字段，重新生成 TS contracts。
- Local/Tauri/Frontend：机器身份文件、profile、runtime start request、ensure payload 不再读取、保存或发送 legacy ids。

