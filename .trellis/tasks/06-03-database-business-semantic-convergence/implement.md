# 实施计划

## Checklist

- [ ] 从父任务报告复制候选清单，按模块排序。
- [ ] 为 Session runtime head 收敛写具体迁移设计。
- [ ] 为 Lifecycle run ledger 收敛写具体迁移设计。
- [ ] 决定 `views` / `user_preferences` 删除或迁移路径。
- [ ] 整理 backend local identity/share/claim 字段边界。
- [ ] 清理业务冗余字段和 contracts。
- [ ] 更新相关 spec。
- [ ] 运行后端 check、前端 typecheck/contract 验证和必要 E2E。

## Candidate Slices

1. Session runtime head 收敛。
2. Lifecycle run projection/audit 拆分。
3. Legacy UI/settings 表迁移。
4. Backend local runtime identity 整理。
5. Business redundancy cleanup。
6. JSONB/credential/permission grant typed cleanup。

## Validation

- `cargo check -p agentdash-infrastructure`
- `cargo check -p agentdash-api`
- contracts 生成/校验命令，以项目现有脚本为准。
- frontend typecheck，以项目现有脚本为准。
- 选择覆盖 Session launch/fork/projection、LifecycleRunView、settings/backend runtime 的关键测试。

