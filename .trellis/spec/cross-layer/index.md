# 跨层契约

> 前后端共享的协议与序列化契约。这些文档不属于单一 layer，而是约束前后端交互边界。

---

## 规范索引

| 文档 | 说明 | 状态 |
|------|------|------|
| [Backbone Protocol](./backbone-protocol.md) | 平台内部事件流统一传输协议（BackboneEnvelope / BackboneEvent） | ✅ 已更新 |
| [Desktop Local Runtime](./desktop-local-runtime.md) | Tauri 桌面端托管 API、Local Runtime command、profile 与打包契约 | ✅ 已创建 |
| [Project Backend Workspace Routing](./project-backend-workspace-routing.md) | Project backend access、workspace inventory、binding 自动路由契约 | ✅ 已创建 |

---

## 归属原则

以下类型的文档应放在此目录：

- 前后端共享的 JSON 序列化契约（字段命名、结构定义）
- 跨 binary（云端/本机）的通信协议
- 前端消费后端事件流的格式约定

如果一个文档**主要**服务于某一端的开发者（即使涉及另一端），应放在对应 layer 目录下。

---

## 相关规范

- [后端开发指南](../backend/index.md)
- [前端开发指南](../frontend/index.md)
- [前后端共享规范](../shared/index.md)
