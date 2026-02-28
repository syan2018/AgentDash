# 会话持久化与多轮对话支持

## 目标

将 AgentDash 的会话模块从「每次发送创建新 session、不可回溯」的摆烂实现，
升级为「持久化、可加载、可接续的多轮对话」体验。

参考实现：`references/ABCCraft`

---

## 核心需求

### P0 – 必须完成

1. **多轮对话**：同一 session 可发送多条 prompt，后端追加而非覆盖
2. **会话加载**：通过 URL `/session/:id` 可加载历史会话
3. **侧边栏可点击**：点击历史会话项跳转到该会话
4. **会话列表 API**：`GET /api/sessions` 从后端读取会话列表
5. **会话接续**：页面刷新后可恢复到之前的会话继续对话

### P1 – 重要

6. **会话元数据**：后端存储 title / createdAt / updatedAt
7. **新建会话流程**：发送第一条消息时自动创建 session 并跳转
8. **删除会话**：侧边栏支持删除操作

### P2 – 可选

9. **Project 隔离**：会话按 project 归属（当前跳过）
10. **会话搜索**：按关键词搜索历史会话（当前跳过）

---

## 技术方案

### 后端变更 (Rust)

#### 1. SessionStore 增强

- 新增 `{session_id}.meta.json` 存储元数据：
  ```json
  { "id": "...", "title": "...", "createdAt": 1234567890, "updatedAt": 1234567890 }
  ```
- `list_sessions()`: 扫描 `.agentdash/sessions/*.meta.json`，返回元数据列表
- `get_session_meta()`: 读取单个会话元数据
- `delete_session()`: 删除 `.jsonl` + `.meta.json`
- `update_meta()`: 更新标题等元信息

#### 2. ExecutorHub 多轮支持

- 移除 `started` 单次保护，允许同一 session 多次 `start_prompt`
- 每次 prompt 不再 `reset()` 文件，改为 **追加**
- 新增 `create_session()` 显式创建会话

#### 3. 新增 API 端点

| Method | Path | Description |
|--------|------|-------------|
| GET    | `/api/sessions` | 列出所有会话（按 updatedAt 倒序） |
| POST   | `/api/sessions` | 创建新会话，返回 `{ id, title, createdAt }` |
| GET    | `/api/sessions/{id}` | 获取会话元数据 |
| DELETE | `/api/sessions/{id}` | 删除会话 |

### 前端变更 (React)

#### 1. 路由

- App.tsx 添加 `<Route path="/session/:sessionId" element={<SessionPage />} />`
- SessionPage 从 `useParams` 获取 sessionId

#### 2. SessionPage 改造

- 移除「每次发送创建新 sessionId」逻辑
- 新会话：创建 session → 跳转到 `/session/{id}` → 发送 prompt
- 已有会话：从 URL 加载 → 显示历史 → 允许继续发送

#### 3. 侧边栏

- 会话列表从后端 API 获取（替换 localStorage）
- 每个会话项可点击跳转
- 当前会话高亮
- 支持删除

#### 4. sessionHistoryStore 重构

- 改为从后端 API 获取数据
- 移除 localStorage persist（或保留为缓存层）
