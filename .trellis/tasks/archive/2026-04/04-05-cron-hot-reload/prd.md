# CronScheduler 动态配置热更新

## Goal

使 CronScheduler 能够在运行时响应 Agent 配置变更，无需重启服务即可生效新的 cron 调度规则。

## Background

当前 `spawn_cron_scheduler` 在服务启动时一次性扫描所有 Agent 配置，然后进入固定的 tick loop。如果用户在运行时修改了某个 Agent 的 `cron_schedule`，必须重启服务才能生效。

对于一个企业级平台，这个限制不可接受——用户期望修改 Agent 配置后立即（或短时间内）生效。

## Requirements

- [ ] CronScheduler 能定期重新扫描 Agent 配置（如每 5 分钟）
- [ ] 新增/修改/删除 Agent 的 cron_schedule 后，无需重启即可反映到调度中
- [ ] 配置无效（如 cron 表达式语法错误）时不影响其他正常条目

## Optional / Future

- [ ] 通过事件总线（而非轮询）监听 Agent 配置变更并即时更新
- [ ] 管理端点查看当前活跃的 cron 调度条目和下次触发时间

## Technical Notes

- 最简方案：在 `run_cron_loop` 中增加周期性的 `load_cron_entries` 调用，与现有 entries 做 diff 更新
- 需要处理 entry 的 next_fire 时间保持——已有的条目不应因 reload 而重置计时
- 可考虑用 entry key (project_id, agent_id) 做幂等 merge

## Acceptance Criteria

- [ ] 修改 Agent cron_schedule 后，5 分钟内调度器自动适配
- [ ] 删除 cron_schedule 后，对应条目停止触发
- [ ] 新增 cron_schedule 后，下一个 reload 周期开始调度
