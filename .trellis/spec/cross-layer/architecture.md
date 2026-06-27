# Cross-layer Architecture

## Role

跨层规范定义前端、云端、本机、桌面壳和共享资产之间的协议、序列化、权限和状态传递边界。它约束的是多个 layer 共同消费的事实，不归属于单一端。

## Invariants

- AgentDash 业务 HTTP JSON 默认使用 `snake_case`。
- 内部 session 事件流统一使用 Backbone Protocol。
- 浏览器实时流统一使用 NDJSON over HTTP，并通过 `x-stream-since-id` 做增量恢复。
- 前后端共享 DTO 由 Rust contract type 生成 TypeScript；生成文件通过 check mode 防止 drift。
- Shared Library / Marketplace / Project Asset 三层表达公共配置资产：运行路径只读取安装后的 Project 资源。
- Dashboard 不直接访问 Tauri/Rust 内存态；Web Dashboard 仍通过 `agentdash-api` HTTP API 访问业务数据。
- Workspace 物理目录识别和文件访问必须经 Runtime Gateway / Local backend，不由云端直接访问本机路径。

## Current Baseline

跨层协议文档：

| 文档 | 当前职责 |
| --- | --- |
| `backbone-protocol.md` | BackboneEnvelope / BackboneEvent wire contract |
| `frontend-backend-contracts.md` | Rust wire DTO、TypeScript 生成、drift check 与迁移优先级 |
| `desktop-local-runtime.md` | Tauri desktop、DashboardHost、LocalRuntimeClient 边界 |
| `deployment-runtime.md` | 云端部署运行入口、环境变量、版本发现和 image command 契约 |
| `project-backend-workspace-routing.md` | Backend Access、workspace detect、inventory registration |
| `shared-library-contract.md` | Shared Library / Marketplace / Project Asset 跨层契约 |

## Local Decisions

- Shared Library payload 在 API 展示层可保留 `unknown` / JSON，但安装和运行前必须由后端按 `asset_type` 类型化校验，原因是共享资产既需要可浏览，也不能让运行路径消费未验证 JSON。
- Desktop Dashboard 等待 `/api/health` ready 后渲染 Web App，原因是 Web Dashboard 的权威接口仍是 HTTP API，而不是 Tauri invoke。
- 业务 DTO 生成使用独立 contract crate 承载 wire type，原因是 API route、前端 generated type 和 drift check 应共享同一个事实源，而不是把路由实现文件当作协议入口。
- 云端 Compose 与 Kubernetes 共用 `agentdash-cloud:<version>` 镜像，原因是单一 image command 契约能让 `serve`、`migrate`、`doctor` 在不同部署形态下保持相同运行语义。

## Contract Appendices

- [Backbone Protocol](./backbone-protocol.md)
- [Frontend / Backend Contracts](./frontend-backend-contracts.md)
- [Desktop Local Runtime](./desktop-local-runtime.md)
- [Deployment Runtime Contract](./deployment-runtime.md)
- [Project Backend Workspace Routing](./project-backend-workspace-routing.md)
- [Shared Library Contract](./shared-library-contract.md)
