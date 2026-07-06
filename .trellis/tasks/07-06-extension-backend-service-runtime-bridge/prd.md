# Extension backendService 本机运行与 bridge 转发闭环

## Goal

在已有 `backend_services` / `fetch_routes` / `operation_catalog` 协议字段基础上，实现 extension `backendService` 的本机 materialize、生命周期管理、bridge 转发与 Agent operation 调用，使带本机后端的 Web App 可以显式声明并随 extension 运行。

## Background

`extension-app-unified-sdk` 已经完成 SDK、toolchain、manifest、domain payload、runtime projection 与 Workspace Module operation catalog 的协议闭环。`backendService` 目前可声明、可打包进 manifest、可投影、可被 Workspace Module 发现，但执行侧保持 fail-closed。

这个状态适合 M0-M2 的协议收束，但还不能完成“既有 Web App 带本机/私有后端导出为 extension”的核心场景。下一步需要把 `backend_services[]` 从声明变成可执行的本机服务，并让 panel fetch route 与 Agent operation 共用同一条 bridge 调用路径。

## Requirements

- Extension package artifact 必须包含 backend service 所需的可执行入口与静态资源，并在 install/runtime 阶段按 manifest digest 和 archive digest 校验。
- Local runtime 必须能够从 Project extension installation 的 package artifact materialize backend service 工作目录。
- Local runtime 必须管理 service lifecycle：start、health、stop、restart、logs、failure diagnostic。
- `fetch_routes[]` 指向 `backendService:<service_key>` 时，panel bridge 必须把请求转发到本机 service，而不是让云端直接访问 localhost。
- Workspace Module invoke 到 `operation_catalog.dispatch.kind = backend_service` 时，必须通过 backend bridge 调用已就绪 service；未就绪时返回结构化 diagnostic。
- 本机 service 的 project、extension、backend、route、trace 信息必须进入调用 metadata，便于审计与排障。
- `backendService` 的执行链路以 manifest 显式声明作为 lifecycle 和 routing 的唯一入口，原因是 localhost/私有后端只能由本机 bridge 安全承载。
- 当前任务只支持 Node runtime 的最小闭环；其它 runtime 属于后续扩展。

## Acceptance Criteria

- [ ] AC1: `agentdash-ext pack` 能把 `backend_services[].entry` 对应服务入口纳入 package，并校验 entry/routes/healthPath。
- [ ] AC2: Local runtime 能 materialize packaged backend service 到 extension artifact cache，并按 extension key + service key 建立 service instance identity。
- [ ] AC3: Local runtime 能启动 Node backend service，记录 endpoint、pid/process handle、health 状态和日志。
- [ ] AC4: health check 失败、进程退出、端口/IPC 不可用时，调用方收到结构化 diagnostic，调用链仍保持 cloud intent + local bridge 执行模型。
- [ ] AC5: Panel `fetch_routes` 到 `backendService:<service_key>` 的请求经 parent bridge 转发到本机 service，并覆盖 GET/POST、headers、body、204/205/304 no-body response。
- [ ] AC6: Workspace Module backendService operation 在 service ready 时可调用，在 service unavailable 时返回稳定错误；`panel_only` 仍不可被 Agent invoke。
- [ ] AC7: Relay/local protocol 中能携带 backend service invoke 请求与响应，metadata 包含 project id、extension key/id、service key、route、backend id、trace id。
- [ ] AC8: 相关 generated contracts 同步，`pnpm run contracts:check` 通过。
- [ ] AC9: 覆盖 toolchain、local runtime、Workspace Module、relay/API focused tests；service not ready 时返回稳定 diagnostic。

## Scope Boundaries

- Service discovery 以 manifest `backend_services[]` 和显式 `fetch_routes[]` 为入口，原因是私有后端访问必须绑定 package、Project installation 与本机 backend 授权事实。
- MVP runtime 为 Node service，原因是当前 `@agentdash/extension` toolchain 与本机 Extension Host 已经围绕 Node/ESM 构建。
- 云端只承载 Project installation、artifact 与 invoke intent，原因是 localhost/私有网络访问发生在 local runtime。
- Workspace Module 通过 operation catalog 调用 backendService bridge，原因是 Agent operation surface 必须保留显式 exposure 与 visibility 约束。
