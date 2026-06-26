# Runner 注册令牌与云端领取流程 - Implement

## Checklist

- 后端领域建模：
  - 新增 runner registration token entity/value object/repository。
  - token 明文只返回一次，数据库只保存哈希。
  - 支持 revoked、expired、last_used_at 状态。
- 数据库迁移：
  - 新增 registration token 表。
  - 索引 project id、token hash、revoked/expiry 查询字段。
  - 跑 migration guard。
- API：
  - 新增项目级 token create/list/revoke/rotate。
  - 新增 runner claim endpoint。
  - claim 成功后创建或更新 backend，并授予 ProjectBackendAccess。
- 权限：
  - 创建、撤销、轮换 token 需要 project backend 管理权限。
  - claim 只接受 registration token，不接受用户 access token。
- 契约生成：
  - 新增 DTO 后运行 contracts generate/check。
  - 前端后续只消费 token 元数据，明文 token 仅创建/轮换响应展示一次。

## Validation

- `pnpm run migration:guard`
- `pnpm run contracts:check`
- `pnpm run backend:check`
- 针对 token 创建、撤销、过期、轮换、claim 成功、claim 失败补充后端测试。

## Risk Points

- Token hash 比较必须使用稳定且不可逆的存储方式。
- Claim 成功后的 backend access 必须严格绑定 token 所属 project。
- 错误响应不能泄露 token 是否接近有效或其他 token 元信息。
