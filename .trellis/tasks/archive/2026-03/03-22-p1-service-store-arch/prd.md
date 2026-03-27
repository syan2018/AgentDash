# Service 层统一与 Store 架构优化 (P1) — 仅追踪

## Goal
统一前端 service 层 API 调用方式，修复 store 中 loading/error 状态互覆盖问题。

## Requirements
1. 统一 services/executor.ts、services/session.ts 使用 api client（同 services/workflow.ts）
2. workflowStore 的 isLoading/error 改为 per-operation 或引入 React Query
3. normalize* 函数策略调整：改为透传 + UI fallback

## Status
仅追踪，待后续排期实施。
