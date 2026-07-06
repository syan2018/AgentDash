# 独立 Runner 自动更新发布链路技术设计草案

## Architecture

runner 自动更新应复用桌面自动更新已经验证过的发布事实流，但安装执行面独立：

```text
主仓 release job
  -> 生成 runner artifacts、signature、sha256
  -> 生成 runner release manifest、channels/stable/latest.json、upload-plan.json

私有发布流程
  -> 上传 versioned runner artifacts
  -> 覆盖 runner stable latest 指针
  -> 将 runner stable manifest HTTP URL 配给云端服务端运行环境

云端服务端 runtime
  -> 读取 AGENTDASH_RUNNER_STABLE_MANIFEST_URL
  -> 校验 manifest schema，短缓存
  -> 合并显式 min_runner_version / recommended_runner_version
  -> 暴露 runner update endpoint

独立 runner
  -> agentdash-local update check/status/install
  -> 校验 signature 与 sha256
  -> 按运行形态执行 drain、replace/install、restart 或输出运维编排提示
```

## Boundaries

- `scripts/` 与 release metadata 脚本拥有 runner artifact manifest、sha256、signature、upload plan 的通用生成职责。
- `crates/agentdash-contracts` 拥有 runner update endpoint DTO。
- `crates/agentdash-api` 拥有 runner update endpoint、manifest URL runtime config、HTTP fetch、schema validation、短缓存和错误映射。
- `crates/agentdash-local` 拥有 runner update CLI、status/doctor 投影、drain-aware install plan、service restart integration。
- 私有仓拥有企业对象存储上传、stable latest URL 注入、service 安装路径和自动更新策略默认值。

## Contracts

### Runner Release Manifest

主仓 manifest 至少表达：

- product、version、git_sha、build_time、channel。
- platforms map，至少覆盖 Windows x86_64 与 Linux x86_64 的自然结构。
- runner artifact：file、kind、object_key、public_url 占位或相对 URL、sha256、signature。
- install strategy hint：`binary_replace`、`archive_replace`、`package_manager`、`container_rollout`。
- latest manifest 只作为 stable 指针，不替代 versioned release manifest。

### Cloud Endpoint

建议 MVP endpoint：

```text
GET /api/runner/update?platform=windows&arch=x86_64&current_version=0.1.0
```

运行期配置草案：

```text
AGENTDASH_RUNNER_STABLE_MANIFEST_URL=<HTTP URL>
AGENTDASH_RUNNER_MANIFEST_CACHE_TTL_SECONDS=60
AGENTDASH_MIN_RUNNER_VERSION=0.1.0
AGENTDASH_RECOMMENDED_RUNNER_VERSION=0.2.0
```

最低版本策略：

- `min_runner_version` 只来自服务端显式运行环境配置。
- 未显式配置时不得触发强制更新或 runner disable。
- `recommended_runner_version` 可由 stable manifest version 驱动，也允许服务端显式配置覆盖。

### Runner CLI / Service

CLI 草案：

```text
agentdash-local update check [--json]
agentdash-local update status [--json]
agentdash-local update install [--dry-run] [--allow-restart]
agentdash-local doctor [--json]
```

运行态策略草案：

```text
AGENTDASH_RUNNER_AUTO_UPDATE=disabled|check_only|install_when_idle
AGENTDASH_RUNNER_UPDATE_DRAIN_TIMEOUT_SECONDS=600
```

## Tradeoffs

- 先使用 runner 专用 endpoint，原因是 runner update response 需要表达 drain、service restart、install strategy 和容器编排提示，过早抽象成通用 client update 会把桌面 DTO 拉胖。
- 默认 `check_only`，原因是 runner 可能正在无人值守执行任务；先提供可诊断更新路径，再通过部署策略逐步开放空闲自动安装。
- 容器场景默认不做进程内自替换，原因是容器镜像版本应由部署编排系统更新，runner 进程只报告当前镜像过旧和所需目标版本。

## Validation Plan

- release metadata 测试覆盖 runner artifact、signature、sha256、stable latest 和 upload plan。
- API 测试覆盖 unconfigured、fetch failed、invalid manifest、unsupported target、update available、min version configured / unconfigured。
- CLI 测试覆盖 check/status/install dry-run、signature/sha256 failure、permission failure、container strategy output。
- runner lifecycle 测试覆盖 active task 时进入 drain 或拒绝安装，空闲时生成 install/restart plan。
- doctor/status 测试覆盖更新策略诊断和最近失败阶段。
