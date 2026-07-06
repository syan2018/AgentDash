# 桌面端自动更新发布链路

## Goal

让 AgentDash 桌面端具备可信的自动更新能力：发布流程能够产出签名的桌面更新包并上传到已有对象存储，云端能够暴露当前可用版本与兼容策略，桌面端能够检查、下载、安装更新，并在版本低于云端最低要求时明确阻止继续使用高风险核心能力。

首个可交付闭环以 Windows stable channel 为 MVP；桌面端第一版固定使用 stable channel，不提供用户选择 channel。后续 macOS、Linux、多 channel 和灰度发布可以在同一 release manifest 结构上扩展。

## Confirmed Facts

- 当前桌面端使用 Tauri 2，配置位于 `crates/agentdash-local-tauri/tauri.conf.json`。
- 当前桌面壳只接入了 `tauri-plugin-single-instance`，尚未接入 Tauri updater 插件。
- 当前桌面发布命令为 `pnpm run desktop:bundle`，会产出 NSIS 安装包。
- 当前发布元数据脚本 `scripts/release-metadata.js` 已记录 `version`、`git_sha`、`build_time`、cloud image 与 desktop installer，但尚未描述 updater artifact、签名、sha256、平台矩阵和对象存储 URL。
- 当前后端版本发现逻辑 `crates/agentdash-api/src/routes/release_info.rs` 已暴露 `min_desktop_version` 与 `recommended_desktop_version` 字段。
- 当前发布 Runbook `deploy/runbooks/release-workflow.md` 已把桌面安装包纳入 artifact inventory，但还没有自动更新发布、签名、对象存储上传和验证流程。
- 当前仓库未发现专用于 release artifact 的对象存储上传抽象；现有 storage 命名主要围绕 Extension package artifact 和本地 filesystem storage。
- 当前 CI release metadata workflow 只上传 GitHub artifact，不执行远端对象存储发布。
- 企业对象存储兼容 AWS S3 协议，可通过 Cyberduck、S3 Browser、s3cmd、AWS CLI 或 AWS SDK 访问；endpoint、bucket、AK/SK、并发设置、上传权限和私有网络访问方式属于企业部署环境事实。
- 云端服务端 release 与桌面端 release 是相对独立的节奏；服务端镜像或二进制打包时不能假设已经能拿到桌面端 stable `latest.json`。
- 云端服务端允许在运行期访问 stable `latest.json` 的公开或内网 HTTP URL。
- 强制更新最低桌面版本由云端服务端运行环境独立控制；stable `latest.json` 只声明当前可下载的推荐/最新桌面版本。
- 当用户打开低于云端 `min_desktop_version` 的旧桌面客户端时，客户端必须进入强制更新阻断态，阻止继续使用依赖兼容协议的核心能力，并引导完成更新。
- 服务端未配置桌面更新策略时不得阻断本地调试：未配置 stable manifest URL 时更新检查应返回未配置/不可用诊断，未显式配置最低桌面版本时不得触发强制更新。
- 强制更新阻断态允许用户查看只读本机诊断/日志，但不得允许启动 runtime、修改设置、进入会话或执行会改变远端/本机运行状态的操作。
- 非强制更新场景下，更新检查失败只在设置页或桌面诊断中展示，不在主界面显示非阻断提醒。
- 项目当前处于预研阶段，不需要保留旧字段或旧更新协议的兼容路径；需求应直接收束到正确的发布模型。

## Requirements

- R1: 桌面发布链路必须产出 Tauri updater 可消费的更新包、签名信息、sha256 和面向人工下载的安装包。
- R2: 发布元数据必须按平台建模，至少覆盖 Windows x86_64 stable channel，并为后续 macOS / Linux 扩展保留自然结构。
- R3: 对象存储只承载不可变版本产物和 channel latest manifest，不作为客户端可信事实源；桌面端必须依赖签名校验确认更新包可信。
- R4: 云端必须提供桌面端可查询的 latest release / update endpoint，返回 stable channel、平台、架构下的最新版本、更新说明、发布时间、下载 URL、签名、sha256、最低可用版本与推荐版本。
- R5: `/.well-known/agentdash` 或同等 discovery 响应中的强制更新最低版本必须来自云端服务端运行环境的显式配置；未显式配置时不得用服务端版本、桌面最新版本或其他默认值触发强制更新。`recommended_desktop_version` 可由 stable latest manifest 的最新可下载版本驱动，并允许服务端环境配置覆盖。
- R6: 桌面端必须固定使用 stable channel，支持用户手动检查更新，并能展示当前版本、最新版本、更新状态、错误信息和重启完成更新入口。
- R7: 桌面端启动后可以自动检查 stable channel 更新，但不得在用户无感知的情况下强制重启应用。
- R8: 当前桌面端版本低于云端 `min_desktop_version` 时，必须展示强制更新状态，并阻止继续使用本机 runtime claim / Relay 连接等依赖兼容协议的核心能力。
- R9: 发布脚本必须能把 updater artifact、签名、release manifest 和 channel latest manifest 上传到已有对象存储或生成可由对象存储发布流程消费的目录结构。
- R10: 更新失败、签名失败、下载失败、安装失败和等待重启状态必须可诊断，至少能在桌面 UI 或本机日志中定位失败阶段。
- R11: 发布 Runbook 必须记录签名密钥职责、对象存储路径、发布顺序、验证步骤和失败排查入口。
- R12: 主仓必须定义对象存储无关的发布目录结构、manifest schema、文件命名、sha256 / signature 产出与上传清单；企业私有子仓负责把这些通用产物映射到具体 S3-compatible endpoint、bucket、凭据、ACL / CDN / 内网访问策略和上传命令。
- R13: 主仓不得记录企业对象存储 endpoint、bucket、AK/SK 或私有访问域名；这些信息只应通过私有子仓、CI secret 或部署环境注入。
- R14: 云端服务端 build artifact 不得内嵌桌面 stable `latest.json`；latest release / update endpoint 必须通过运行期配置发现桌面 stable manifest，允许桌面端 release 在不重打服务端镜像的情况下更新可用版本。
- R15: 云端 latest release / update endpoint 的运行期数据源应是对象存储或 CDN 上稳定地址的 stable `latest.json`，服务端只配置 manifest URL 并按需读取、校验和短缓存，不持有对象存储 AK/SK。
- R16: 桌面端只通过 AgentDash 云端 latest release / update endpoint 获取 stable 更新信息，不直接读取对象存储 latest manifest；云端负责读取 manifest、校验 schema、短缓存并返回桌面端可消费响应。
- R17: 强制更新阻断态必须优先于本机 runtime 自动连接、登录后的 runtime claim、Relay 连接和会话入口；旧客户端只应保留检查更新、下载更新、安装更新、重试、退出和必要诊断能力。
- R18: 本地开发和未配置发布策略的服务端必须可正常启动和使用桌面端核心开发链路；缺少 stable manifest URL、推荐版本或最低版本配置时，只影响更新检查能力，不触发强制阻断。
- R19: 强制更新阻断态中的诊断能力必须是只读的，允许查看当前客户端版本、服务端最低版本、manifest URL 配置状态、manifest 拉取/校验错误、下载/签名/安装错误和本机日志，不允许产生运行侧副作用。
- R20: 非强制更新场景下，更新检查失败不得打断主界面使用；错误状态只应在设置页、桌面更新区域或诊断日志中呈现。

## Non-Goals

- 不在 MVP 中实现 channel 选择、多 channel 灰度比例、企业分组策略或 A/B 发布。
- 不在 MVP 中支持旧版本更新协议或旧 release metadata 字段兼容。
- 不在 MVP 中要求 macOS / Linux 自动更新闭环，但 manifest 结构不能把 Windows 写死到顶层事实模型。
- 不把对象存储凭据下发到桌面端。
- 不在主仓实现绑定某个企业对象存储实例的上传流程；主仓只提供可被私有发布流程消费的标准产物和契约。

## Task Organization

- 私有仓对象存储部署适配单独拆为子任务 `07-06-desktop-update-private-deployment`。
- 主仓实现保留在当前父任务中，按 `work-items/` 下的工作项文件逐个实现和 check。
- 主仓工作项：
  - `work-items/01-release-artifacts-manifest.md`：发布产物与 manifest 契约。
  - `work-items/02-cloud-update-endpoint.md`：云端 latest release / update endpoint。
  - `work-items/03-desktop-updater-flow.md`：桌面 updater 流程。
  - `work-items/04-force-update-gate-diagnostics.md`：强制更新阻断与只读诊断。

## Acceptance Criteria

- [ ] 使用 release 命令能够构建 Windows 桌面安装包和 Tauri updater artifact，并生成对应签名与 sha256。
- [ ] 发布元数据输出包含 `product`、`version`、`git_sha`、`build_time`、`channel`、平台矩阵、人工安装包、updater artifact、signature、sha256、对象存储 URL 或可上传对象路径。
- [ ] 对象存储发布产物采用不可变版本目录与 stable channel latest 指针；客户端不依赖 bucket listing。
- [ ] 云端 latest release / update endpoint 能按 stable channel、platform、arch、current_version 返回桌面端可消费的更新信息。
- [ ] 发布新桌面版本并更新对象存储 stable `latest.json` 后，不需要重新构建服务端镜像即可让云端 latest release / update endpoint 返回新版本。
- [ ] 服务端未配置 desktop stable manifest URL 或读取失败时，endpoint 返回可诊断错误，不影响 `/api/version`、`/.well-known/agentdash` 的基础版本发现，也不阻断未触发显式最低版本策略的本地调试。
- [ ] 桌面端更新检查只调用 AgentDash 云端 API，不需要配置对象存储 latest URL。
- [ ] 桌面端在有新版本时能展示可更新状态，下载并安装签名正确的更新包，随后提示重启完成更新。
- [ ] 篡改 updater artifact 或 signature 时，桌面端拒绝安装并展示可诊断错误。
- [ ] 当云端 `min_desktop_version` 高于当前桌面端版本时，桌面端显示强制更新提示，并阻止继续使用需要协议兼容的核心连接能力。
- [ ] 未显式配置云端最低桌面版本时，桌面端不会因为缺少更新策略、缺少 latest manifest 或服务端/桌面版本不一致而进入强制更新阻断态。
- [ ] 低于云端 `min_desktop_version` 的旧桌面端启动后不会自动连接本机 runtime，不会发起 runtime claim，不会建立 Relay 连接，也不能进入需要协议兼容的会话工作流。
- [ ] 强制更新阻断态允许打开只读诊断/日志视图，并确认这些入口不会启动 runtime、修改设置、建立 Relay 或进入会话。
- [ ] 非强制更新检查失败时，主界面不显示提醒；设置页或诊断视图能看到失败阶段与错误原因。
- [ ] 当云端仅 `recommended_desktop_version` 高于当前版本时，桌面端显示非阻断更新提示。
- [ ] 更新流程不会静默重启正在使用的桌面端。
- [ ] 发布 Runbook 覆盖自动更新发布、对象存储上传、签名密钥、验证和排查流程。
- [ ] 主仓生成的发布目录可以被 AWS CLI / s3cmd / 私有子仓脚本直接同步到 S3-compatible 对象存储。
- [ ] 主仓文档只描述对象存储通用契约和所需环境变量占位，不包含企业 endpoint、bucket、AK/SK 或私有域名。
- [ ] 相关检查通过：`pnpm run desktop:check`、更新相关 Rust 测试、更新相关 TypeScript 类型检查、release metadata 脚本测试。

## Open Questions

- 当前没有阻塞 PRD 的开放问题；后续进入实现前需要补 `design.md` 和 `implement.md`。
