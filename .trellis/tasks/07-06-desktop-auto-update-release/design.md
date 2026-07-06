# 桌面端自动更新发布链路技术设计

## Architecture

本任务在主仓内建立一条对象存储无关的桌面自动更新链路。主仓负责生成可信发布产物、云端 update endpoint、桌面 updater 体验和强制更新闸门；企业 S3-compatible 对象存储上传、endpoint、bucket、AK/SK、ACL、CDN 或内网域名由子任务 `07-06-desktop-update-private-deployment` 对应的私有仓流程承接。

整体数据流：

```text
主仓 release job
  -> 生成 Windows 安装包、Tauri updater artifact、signature、sha256
  -> 生成 version release manifest、channels/stable/latest.json、upload-plan.json

私有仓发布流程
  -> 消费 upload-plan.json
  -> 上传 versioned artifacts
  -> 覆盖 channels/stable/latest.json
  -> 将 stable latest manifest HTTP URL 配给云端服务端运行环境

云端服务端 runtime
  -> 读取 AGENTDASH_DESKTOP_STABLE_MANIFEST_URL
  -> 校验 manifest schema，短缓存
  -> 合并显式 min_desktop_version / recommended_desktop_version
  -> 暴露桌面 latest release / update endpoint

桌面端
  -> 只调用 AgentDash 云端 update endpoint
  -> 用 Tauri updater 校验签名、下载、安装
  -> 低于显式 min_desktop_version 时进入强制更新阻断态
```

## Boundaries

- `scripts/` 与 release metadata 脚本拥有通用发布目录、manifest、sha256、upload plan 的生成职责。
- `crates/agentdash-contracts` 拥有浏览器/桌面需要消费的 update endpoint DTO；`packages/app-web/src/generated` 由 contract 生成。
- `crates/agentdash-api` 拥有 update endpoint、manifest URL runtime config、HTTP fetch、schema validation、短缓存和错误映射。
- `crates/agentdash-local-tauri` 拥有 Tauri updater 插件、桌面命令、启动早期强制更新闸门和本机 runtime auto-connect gating。
- `packages/app-tauri` 和复用的 Web UI 入口拥有桌面更新 bridge、设置页更新状态、强制更新阻断 UI 和只读诊断入口。
- 私有仓拥有企业对象存储上传实现；主仓不持有企业真实 endpoint、bucket、AK/SK 或私有域名。

## Contracts

### Release Manifest

主仓 manifest 必须表达：

- product、version、git_sha、build_time、channel。
- platforms map，首版至少覆盖 `windows-x86_64`。
- 人工安装包 artifact：file、object_key、public_url 占位或相对 URL、sha256。
- updater artifact：file、object_key、public_url 占位或相对 URL、sha256、signature。
- latest manifest 只作为 stable 指针，不替代 versioned release manifest。

### Cloud Endpoint

云端 endpoint 运行期读取 stable latest manifest HTTP URL。服务端 build artifact 不内嵌桌面 latest manifest。

最低版本策略：

- `min_desktop_version` 只来自服务端显式运行环境配置。
- 未显式配置时不得触发强制更新。
- `recommended_desktop_version` 可由 stable latest manifest version 驱动，也允许服务端显式配置覆盖。

未配置策略：

- 未配置 stable manifest URL 时 update endpoint 返回可诊断状态。
- 该状态不影响 `/api/version`、`/.well-known/agentdash` 基础发现，不阻断本地调试。

### Desktop Gate

强制更新闸门优先于：

- 本机 runtime auto-connect。
- 登录后的 runtime claim。
- Relay 连接。
- 需要协议兼容的会话入口。

阻断态保留：

- 检查更新、下载更新、安装更新、重试、退出。
- 只读诊断/日志。

阻断态禁止：

- 启动 runtime。
- 修改设置。
- 建立 Relay。
- 进入会话或触发运行侧副作用。

## Tradeoffs

- 主仓不直接上传企业对象存储，原因是企业 endpoint、bucket、凭据、ACL 和访问域名属于私有部署事实；主仓用 upload plan 保持通用发布契约。
- 服务端运行期读取 stable latest URL，原因是服务端与桌面端 release 节奏独立；更新桌面 stable 指针不应要求重打服务端镜像。
- 桌面端不直连对象存储 latest manifest，原因是更新策略、强制更新和诊断应收束在云端 API。
- 强制更新必须由显式最低版本配置触发，原因是本地调试和未配置部署环境不能因缺省策略被锁死。

## Operational Notes

- 发布顺序必须先上传 versioned artifacts，再覆盖 stable latest 指针。
- stable latest 指针回滚由私有仓发布流程处理；主仓需要保证 manifest 和 upload plan 能支持回滚。
- Tauri updater 签名私钥只应存在于 release 环境；公钥进入桌面端配置。
- 非强制更新失败只进入设置页/诊断日志，不在主界面打扰用户。

## Work Item Map

- `work-items/01-release-artifacts-manifest.md`：发布产物与 manifest 契约。
- `work-items/02-cloud-update-endpoint.md`：云端 latest release / update endpoint。
- `work-items/03-desktop-updater-flow.md`：桌面 updater 流程。
- `work-items/04-force-update-gate-diagnostics.md`：强制更新阻断与只读诊断。
