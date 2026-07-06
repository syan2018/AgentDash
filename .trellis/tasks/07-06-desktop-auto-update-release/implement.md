# 桌面端自动更新发布链路实施计划

## Order

1. 发布产物与 manifest 契约
   - 接入 Tauri updater artifact 产出。
   - 扩展 release metadata 为平台矩阵。
   - 生成 version manifest、stable latest manifest、sha256 和 upload plan。
   - 为 release metadata / manifest schema 补测试。

2. 云端 latest release / update endpoint
   - 在 contract crate 增加 update endpoint DTO。
   - 在 API 增加运行期 stable manifest URL 配置、HTTP fetch、schema validation、短缓存和错误映射。
   - 确保未配置 stable manifest URL 不阻断本地调试。
   - 确保 `min_desktop_version` 只来自显式运行环境配置。

3. 桌面 updater 流程
   - 接入 Tauri updater 插件和前端 bridge。
   - 在桌面设置区域展示更新状态和手动检查入口。
   - 支持启动后自动检查 stable 更新。
   - 非强制更新失败只进设置页/诊断日志。

4. 强制更新阻断与只读诊断
   - 在桌面启动早期获取服务端版本策略。
   - 在低于显式 `min_desktop_version` 时阻断 runtime auto-connect、runtime claim、Relay 和会话入口。
   - 保留更新、重试、退出和只读诊断。
   - 验证未配置最低版本时本地调试不被阻断。

5. 文档与收尾
   - 更新 release Runbook。
   - 更新部署 runtime contract 或相关 spec。
   - 跑质量检查。
   - 提交主仓实现。

## Parallelization

- 工作项 1 和工作项 2 可以部分并行：manifest schema 草案确定后，云端 endpoint 可先基于 fixture 开发。
- 工作项 3 和工作项 4 可以部分并行：桌面 bridge/update 状态机与强制更新 gate 可分开实现，最后在启动链路集成。
- 私有仓部署适配由子任务独立推进，不阻塞主仓代码实现；主仓只需稳定 upload plan 与 manifest schema。

## Validation Commands

- `pnpm run release:metadata`
- release metadata / manifest 相关脚本测试
- `pnpm run contracts:check`
- `pnpm run backend:check`
- `pnpm run desktop:check`
- 强制更新 gate 相关 Rust / TypeScript 单元测试

## Risk Points

- Tauri updater artifact 与当前 `desktop:bundle --no-sign` 参数可能需要重新确认产物名称和签名流程。
- update endpoint manifest schema 如果直接服务 Tauri updater，需要确认 Tauri 2 updater endpoint 期望字段；必要时云端 endpoint 做 manifest 转换。
- 强制更新 gate 必须早于 runtime auto-connect，否则旧客户端会在被阻断前已经发起 claim 或 Relay。
- 未配置策略必须是非阻断路径，避免 `pnpm dev` 和本地桌面调试被锁死。
- 主仓 manifest 不得泄露企业对象存储真实 endpoint、bucket、AK/SK 或私有域名。

## Ready To Start Checklist

- [x] PRD 已收敛，无阻塞 open question。
- [x] 私有仓部署适配已拆为子任务。
- [x] 主仓工作项文件已创建。
- [x] `design.md` 已覆盖边界、数据流和 tradeoff。
- [x] `implement.md` 已覆盖顺序、并行策略和验证命令。
- [x] `implement.jsonl` 和 `check.jsonl` 已配置真实 spec context。
- [ ] 用户确认进入实现，或当前请求明确要求开始处理父任务。
