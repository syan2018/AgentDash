# 状态管理

> Zustand 5 全局状态 + React useState 本地状态。

---

## 状态分层

| 状态类型 | 存放位置 | 示例 |
|----------|----------|------|
| 本地 UI 状态 | 组件内 `useState` | `isOpen`, `selectedTab` |
| Feature 状态 | Feature `model/` hooks | `entries`, `isConnected` |
| 全局应用状态 | `stores/` | `projects`, `currentProjectId` |
| 服务端缓存 | Store + API | `tasksByStoryId`, `workspacesByProjectId` |

派生状态使用 `useMemo` 计算，不存储在状态中。

---

## Store 清单

| Store | 职责 |
|-------|------|
| `projectStore` | Project CRUD + 选择 |
| `workspaceStore` | Workspace CRUD + 状态管理 |
| `storyStore` | Story/Task 数据 |
| `coordinatorStore` | 后端连接管理 |
| `eventStore` | 项目级 NDJSON 事件流 |
| `workflowStore` | Workflow 管理 |
| `sessionHistoryStore` | 会话历史 |
| `settingsStore` | 全局设置 |
| `currentUserStore` | 当前用户 |
| `activeSessionsStore` | 活跃会话追踪 |
| `llmProviderStore` | LLM Provider 管理 |
| `routineStore` | Routine 管理 |
| `authStore` | 认证状态 |
| `sidebarSessionsStore` | 侧边栏会话列表 |
| `workspaceTabStore` | 工作空间标签页 |

---

## 何时使用全局 Store

1. **跨组件共享**：多个不相关组件需要访问同一份数据
2. **跨页面持久**：路由切换后仍需保持的状态
3. **服务端缓存**：从 API 获取的数据需要缓存

---

## Store 规范

- 使用 `isLoading` / `error` 追踪加载和错误状态
- 内部 API response 由 service 层按 generated contract type 返回；store 不为 generated DTO 再做字段级归一化
- Store state 消费 service 层产出的 typed DTO 或 view model；跨层 DTO 类型来自 `src/generated/*`，原因是 store 不应成为协议字段事实源
- 按 Feature 拆分 Store，避免单个 Store 过大
- 始终通过 `set` 更新状态，不直接修改

---

## Projection Store 写后刷新

HTTP-only projection store（如 `extensionRuntimeStore` 缓存的 `ExtensionRuntimeProjectionResponse`）没有 SSE / NDJSON 失效流。**任何会改变底层实体的写操作（HTTP POST/DELETE 等），调用方必须在 success 分支显式调 `store.fetchProject(projectId)` 触发重拉**，不能依赖局部 patch 或 optimistic update。

**为什么**：projection 由后端聚合多张表（installation / artifact / runtime action / workspace tab / permission / bundle）派生，前端无法本地推导；漏 refetch 会造成"写完了但 UI 还是旧数据"，或更糟：不同入口看到的投影不一致。

**典型形态**（写入处复制此模式）：

```ts
async function handleUninstall() {
  setBusy(true);
  try {
    await uninstallExtensionInstallation(projectId, installationId);
    await useExtensionRuntimeStore.getState().fetchProject(projectId); // 必填
    setNotice({ tone: "success", message: "已更新 Extension runtime projection" });
  } catch (err) {
    setNotice({ tone: "danger", message: extractMessage(err) });
  } finally {
    setBusy(false);
  }
}
```

适用范围：写后无 stream invalidation 的 store。如果 store 已订阅事件流（`eventStore`、`sessionHistoryStore` 这类），由 reducer 接管失效，不需要手动 refetch。新建此类 store 时把"写操作的入口在哪里 fetch"写在 store 顶部注释里，避免漏配。

---

## 常见错误

| 错误 | 正确做法 |
|------|----------|
| 在多个 Store 存储同一份数据 | 单一 Store 存储，其他使用 selector |
| 存储可计算数据 | 使用 `useMemo` 计算 |
| 直接修改状态 | 始终通过 `set` 更新 |
| Store 过于庞大 | 按 Feature 拆分 |
| 忘记 reset 状态 | 提供 reset action |
