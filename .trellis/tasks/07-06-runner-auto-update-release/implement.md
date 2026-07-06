# 独立 Runner 自动更新发布链路实施计划草案

## Order

1. 需求收敛
   - 确认 MVP 默认自动安装策略。
   - 确认低于最低版本时对已开始任务的处理方式。
   - 确认是否先做 runner 专用 endpoint。

2. Runner 发布产物契约
   - 扩展 release metadata，加入 runner artifact platform matrix。
   - 生成 runner stable latest manifest 与 upload plan。
   - 为 runner artifact signature / sha256 补测试。

3. 云端 runner update endpoint
   - 在 contract crate 增加 runner update DTO。
   - 在 API 增加 runner stable manifest URL runtime config、HTTP fetch、schema validation、短缓存和诊断状态。
   - 确保未配置策略不阻断本地 runner 开发。
   - 确保 `min_runner_version` 只来自显式环境配置。

4. agentdash-local update CLI
   - 增加 `update check/status/install`。
   - 实现下载、signature、sha256、install plan 和 dry-run。
   - 将最近更新状态纳入 `doctor` / `status`。

5. Service-aware drain / restart
   - 识别当前 runner 是否有活动执行。
   - 空闲时允许 install plan 进入替换/重启。
   - 忙碌时进入 drain 或返回需要人工确认的状态。
   - 区分 systemd、Windows Service、裸二进制、容器策略。

6. 文档与私有部署拆分
   - 更新 deployment runtime / desktop-local-runtime 或 runner spec。
   - 拆出企业对象存储与 service 部署适配子任务。
   - 跑质量检查并提交。

## Parallelization

- 发布产物契约和云端 endpoint 可以并行，前提是先冻结 runner manifest fixture。
- CLI status/check 可以与 install/drain 分开实现；install/drain 风险更高，建议后置。
- 私有部署适配独立推进，不阻塞主仓 DTO、manifest 和 CLI 契约。

## Validation Commands

- `pnpm run release:metadata`
- runner release metadata 脚本测试
- `pnpm run contracts:check`
- `cargo test -p agentdash-api runner_update --lib`
- `cargo test -p agentdash-local update`
- `cargo check -p agentdash-local`
- `pnpm run backend:check`

## Risk Points

- 进程自替换在 Windows 和 systemd service 下权限差异很大，必须先 dry-run/install plan，再逐步打开自动安装。
- 正在执行任务时更新可能破坏 session 或 relay lease，必须先定义 drain 行为。
- 容器部署不应鼓励容器内 binary 自替换，状态诊断和 orchestration hint 要作为一等策略。
- `min_runner_version` 和 relay protocol compatibility 的关系要清晰，避免把推荐升级误判为强制停止接任务。
