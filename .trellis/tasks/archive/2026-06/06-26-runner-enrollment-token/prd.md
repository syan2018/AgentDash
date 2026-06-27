# Runner 注册令牌与云端领取流程

## Goal

为独立 Local Runner 提供无 UI 注册与领取能力，使服务器托管场景不需要保存用户 access token，也不需要手动传入长期 `backend_id`/`auth_token`。

## Requirements

- 云端提供 runner registration token 创建、查看、撤销、轮换能力。
- Registration token 可绑定明确作用域；第一版至少支持用户或项目范围中的一种，并在本任务 design 中固定。
- Runner 使用 registration token 调用云端领取流程，领取 `backend_id`、`relay_ws_url`、`auth_token`、machine 信息和 capability slot。
- 现有桌面端基于用户 access token 的 `/api/local-runtime/ensure` 路径继续存在，但授权路径、DTO 命名和错误提示需要与 runner token 区分。
- Registration token 必须支持过期或撤销策略，避免服务器遗留 token 永久有效。
- 云端 backend/project 可见性与 registration token 授权范围保持一致。

## Acceptance Criteria

- [ ] API 测试覆盖 token 创建、撤销、过期、重复使用策略。
- [ ] `/api/local-runtime/ensure` 或新的领取入口能区分桌面 access token 与 runner registration token。
- [ ] Runner 使用 registration token 能完成无 UI backend 领取并获得 relay 连接凭据。
- [ ] 无效、过期、撤销、作用域不匹配的 token 返回明确错误。
- [ ] 数据库迁移包含 registration token 所需持久化结构，并通过 migration guard。

## Notes

- 这是 runner 服务化的前置子任务。
- 启动前补齐本子任务 `design.md` 与 `implement.md`，并明确 token 哈希存储、一次性/长期 token 策略和作用域模型。
