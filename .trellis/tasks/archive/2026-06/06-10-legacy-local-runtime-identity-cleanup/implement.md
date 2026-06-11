# Implement Plan

1. 删除任务范围内所有 `legacy_machine_ids` 字段定义与调用点。
2. 收窄 PostgreSQL local backend ensure 查询：只匹配当前 `machine_id + scope + slot`，不做 legacy candidate/fallback/duplicate merge。
3. 修改初始化 migration 并按授权运行 migration guard。
4. 重新生成 frontend contracts，清理前端消费和测试 fixtures。
5. 运行 targeted Rust / TS 检查：migration guard、contracts check、相关 crate check、frontend check。
