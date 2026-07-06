# 独立 Runner 自动更新发布链路

## Goal

让独立 `agentdash-local` runner 具备可信、可诊断、可运维控制的自动更新能力。runner 发布流程应复用主仓通用 release manifest、signature、sha256、stable channel 和云端 update policy 思路，但安装执行面必须独立于 Tauri desktop updater，因为独立 runner 通常以 systemd、Windows Service、裸二进制或容器方式运行。

首个讨论目标是收敛 MVP 边界：runner 如何发现新版本、何时允许自动安装、如何避免打断正在执行的任务、低于最低版本时如何停止接新任务，以及哪些对象存储/服务管理细节应留给私有部署任务。

## Confirmed Facts

- 桌面自动更新父任务 `07-06-desktop-auto-update-release` 已建立主仓通用模式：release metadata 产出 manifest / stable latest / upload plan，云端运行期读取 stable manifest URL，客户端只调用云端 update endpoint，不直接读取对象存储。
- 独立 runner 属于 `agentdash-local` 执行面，不运行 Tauri，也不能直接使用 `tauri-plugin-updater`。
- 独立 runner 可能由 `agentdash-local run`、`agentdash-local setup`、systemd、Windows Service 或容器部署拉起。
- runner 可能正在执行 relay session、tool、MCP、extension host 或 workspace 操作；更新安装不能无条件杀进程。
- runner enrollment / relay 鉴权和 ProjectBackendAccess 已有独立任务历史，registration token 与 runtime relay auth 的生命周期不同。
- 企业对象存储部署事实仍应留在私有仓、CI secret 或运行环境中；主仓只定义对象存储无关的 artifact / manifest / upload plan 契约。
- 本地开发和未配置更新策略的部署环境不得因为缺少 runner stable manifest 或最低版本配置而被锁死。

## Requirements

- R1: 主仓 release metadata 必须能表达独立 runner artifact，至少覆盖 Windows x86_64 与 Linux x86_64 的自然扩展结构，并保留 macOS / 多架构扩展空间。
- R2: runner artifact 必须包含 signature 与 sha256；runner 更新安装必须在替换或执行安装动作前完成可信校验。
- R3: 云端必须提供 runner 可查询的 update policy endpoint，返回 stable channel、platform、arch、current_version、latest version、release notes、published_at、download URL、signature、sha256、最低可用 runner 版本、推荐 runner 版本和诊断信息。
- R4: runner update endpoint 的运行期数据源必须是 stable manifest HTTP URL；server build artifact 不内嵌 runner latest manifest，允许 runner release 独立推进。
- R5: `min_runner_version` 必须来自云端服务端显式运行环境配置；未显式配置时不得从 server version、runner latest version 或 relay protocol version 推导强制更新。
- R6: `agentdash-local` 必须提供可运维入口：`update check`、`update status`、`update install`，并让 `doctor` 展示当前版本、最新版本、最低版本要求、更新策略配置状态和最近失败阶段。
- R7: 自动安装策略必须可配置，MVP 至少区分 `disabled`、`check_only`、`install_when_idle`；默认策略需要偏保守，避免无人值守环境中意外重启正在工作的 runner。
- R8: runner 正在执行任务或持有活动 lease 时，自动安装不得直接替换进程；必须进入 drain、等待空闲或返回需要人工确认的状态。
- R9: 低于显式 `min_runner_version` 的 runner 不应继续接新任务或建立会破坏协议兼容性的运行能力；但必须保留 update / status / doctor 等恢复入口。
- R10: systemd 与 Windows Service 场景必须有明确 restart path；裸二进制场景必须能输出安装计划；容器场景可以报告镜像版本过旧并交给部署编排更新。
- R11: runner 更新失败必须可诊断，至少区分 policy fetch、manifest validation、download、signature、sha256、drain timeout、permission、replace/install、restart 等阶段。
- R12: 私有仓或后续部署任务负责企业对象存储 endpoint、bucket、AK/SK、ACL、CDN/内网访问策略、service 安装路径、自动安装开关默认值和容器 rollout 策略。

## Technical Notes

- 推荐先把 endpoint 命名为 runner 专用 `/api/runner/update`，等 desktop 与 runner DTO 稳定后再考虑抽象成通用 `/api/client/update`，原因是 runner update policy 包含 drain/service/install strategy，和桌面 Tauri updater 的返回语义不同。
- runner artifact 与 desktop artifact 可以共享 release manifest 的平台矩阵思想，但 artifact kind、安装策略和签名校验实现应独立建模。
- runner 的强制更新应进入 `update_required` / `disabled_until_update` / `draining_for_update` 等状态，而不是复用桌面 UI 阻断语义。
- 容器 runner 的更新语义应以诊断和 orchestration hint 表达，原因是容器内替换自身 binary 通常不是正确的部署模型。

## Non-Goals

- 不在本任务中复用 Tauri updater 或桌面 UI 阻断屏。
- 不在主仓记录企业对象存储 endpoint、bucket、AK/SK 或私有域名。
- 不在 MVP 中实现多 channel、灰度比例、企业分组策略或 A/B 发布。
- 不在 MVP 中要求所有安装形态都能进程内自替换；容器和权限受限环境可以走运维编排路径。
- 不把 registration token 当作 relay auth 或 update auth；更新发现与 runner enrollment / relay authentication 保持独立生命周期。

## Task Organization

- 本任务为独立 planning 任务，不作为桌面自动更新父任务的子任务。
- 桌面自动更新任务只作为可复用契约来源；runner 自动更新需要独立 PRD / design / implement 收敛后再进入实现。
- 企业对象存储与 service 部署适配可在本任务收敛后再拆私有部署子任务。

## Acceptance Criteria

- [ ] PRD 明确 runner update MVP 的安装策略、drain 行为、强制最低版本语义和未配置策略。
- [ ] 设计文档明确主仓通用发布契约与 runner-specific install strategy 的边界。
- [ ] release metadata 能表达 runner artifact、signature、sha256、platform/arch、object key 或 public URL 占位。
- [ ] 云端 runner update endpoint 能在未配置 stable manifest URL 时返回可诊断状态，不影响本地 runner 开发链路。
- [ ] `min_runner_version` 未显式配置时，runner 不会进入强制更新/disabled 状态。
- [ ] 低于显式 `min_runner_version` 的 runner 不接新任务，但保留 update/status/doctor 恢复能力。
- [ ] `agentdash-local update check/status/install` 的 CLI 契约被设计并覆盖测试计划。
- [ ] systemd、Windows Service、裸二进制、容器四种运行形态都有明确的 MVP 行为或非目标说明。
- [ ] 私有部署职责被单独列出，不把企业对象存储或 service 安装事实写进主仓代码/文档。

## Open Questions

- O1: MVP 默认自动安装策略应是 `check_only` 还是 `install_when_idle`？推荐 `check_only`，原因是独立 runner 可能承载无人值守执行任务，先保证可诊断和人工确认，再开放空闲自动安装。
- O2: 低于 `min_runner_version` 时是否允许继续完成已经开始的任务？推荐允许已开始任务完成、停止接新任务，原因是中途杀进程比协议兼容风险更难恢复；只有云端明确声明 hard block 时才立即断开。
- O3: runner update endpoint 是否一开始就做成通用 client update endpoint？推荐先 runner 专用，原因是 runner 的 drain/service/install strategy 会让通用 DTO 过早变胖。
