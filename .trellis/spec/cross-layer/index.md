# 跨层契约索引

跨层 spec 约束前端、云端、本机、桌面壳和共享资产之间共同消费的协议与事实。

## Architecture Entry

- [Cross-layer Architecture](./architecture.md)

## Contract Appendices

| 文档 | 说明 |
| --- | --- |
| [Backbone Protocol](./backbone-protocol.md) | 内部 session 事件流协议 |
| [Frontend / Backend Contracts](./frontend-backend-contracts.md) | Rust wire DTO、TS 生成与 drift check 契约 |
| [Desktop Local Runtime](./desktop-local-runtime.md) | Tauri 桌面壳、DashboardHost、LocalRuntimeClient 边界 |
| [Deployment Runtime Contract](./deployment-runtime.md) | 云端部署运行入口、环境变量、版本发现和 image command 契约 |
| [Project Backend Workspace Routing](./project-backend-workspace-routing.md) | Backend Access、workspace detect、inventory registration |
| [Shared Library Contract](./shared-library-contract.md) | Shared Library / Marketplace / Project Asset 跨层契约 |

## 归属原则

以下内容放在 cross-layer：

- 前后端共享的 JSON / NDJSON 序列化契约。
- 跨 cloud/local/desktop 的通信边界。
- 多端共同消费的状态流、权限语义和来源元数据。

如果文档主要服务于某一端，即使涉及另一端，也应放到对应 layer 的 architecture 或 appendix 下，并从这里链接。
