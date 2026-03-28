# 前端项目隔离与会话恢复语义修复

## 目标

- 前端实体缓存按 `projectId` 或 `sessionId` 正确隔离。
- ACP reconnect / remount 时不清空已有 transcript。
- 拆解超大页面，降低回归半径。

## 非目标

- 不在本任务内做视觉改版。
- 不追求一次性重写全部 store。

## 验收标准

- `Story` 相关列表和任务数据切项目时不互相污染。
- 重连时 UI 不会先闪空再等待回放。
- `SessionPage` / `StoryPage` 至少完成第一轮按 feature 拆分。
