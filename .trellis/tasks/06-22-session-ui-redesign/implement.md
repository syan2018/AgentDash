# Card Body 统一重构执行计划

## Phase 1 — 外壳重构 ✅

已完成：消息去卡片化、strip 模式、聚合折叠、事件降噪、轮次折叠、间距收窄。

## Phase 2 — Card Body 统一设计语言

### 2.1 提取设计 token 常量 ✅
- [x] 创建 `cardBodyTokens.ts`，导出所有共享样式常量

### 2.2 CommandExecutionCardBody ✅
- [x] 输出块容器 → `CB.codeBlock`
- [x] footer 按钮 → `CB.actionButton`
- [x] exit code 着色 → token 体系

### 2.3 ReadCardBody ✅
- [x] 代码容器 → `CB.inlineEntry`
- [x] 行号 → `CB.lineNumber`
- [x] 复制/展开按钮 → `CB.actionButton`

### 2.4 FileChangeCardBody ✅
- [x] 文件条目 → `CB.inlineEntry` + `CB.inlineEntryButton`
- [x] diff 展开容器统一
- [x] kind badge → `CB.kindBadge`

### 2.5 McpCardBody + GenericJsonBody ✅
- [x] 分区标题 → `CB.sectionTitle`
- [x] error 块 → `CB.errorBlock`

### 2.6 ContextFrame 对齐 TOOLS ✅
- [x] 展开面板 → `CB.expandPanel`
- [x] tab 样式轻量化

### 2.7 其余 body ✅
- [x] WebSearchCardBody
- [x] CollabAgentCardBody
- [x] ImageCardBody
- [x] TaskToolCardBody

### 2.8 验证 ✅
- [x] `pnpm --filter app-web run typecheck` — 通过
- [x] `pnpm --filter app-web run lint` — 无新增错误

## 回滚点

Phase 2 各步独立提交，任一步骤出问题可 `git revert` 单步。
