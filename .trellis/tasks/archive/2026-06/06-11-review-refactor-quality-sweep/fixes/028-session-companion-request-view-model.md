# 028 Session Companion Request View Model

## 问题

`SessionCompanionRequestCard` 同时负责解包 `BackboneEvent` / `payload`、解释 companion request 展示字段、处理 capability grant 细节与组装响应 payload，导致 UI 组件承担过多 model 逻辑。

## 改动

- 新增 `companionRequestViewModel`，集中解析 requestId、gateId、payloadType、uiHint、prompt、options、wait、capability grant 展示信息、badge、detail lines 与 debug chips。
- 将普通 choice 响应和 capability grant 响应 payload 组装移动到 model helper，保留原有 `decision` 与 `capability_grant_result` payload 结构。
- `SessionCompanionRequestCard` 保留本地提交状态、错误状态、`respondCompanionRequest` 调用和渲染分支。

## 涉及文件

- `packages/app-web/src/features/session/model/companionRequestViewModel.ts`
- `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx`

## 验证

- `pnpm --filter app-web test -- SessionSystemEventCard.test.tsx`：通过，1 个测试文件 / 6 个测试通过。
- `pnpm --filter app-web run typecheck`：通过，`tsc --noEmit -p tsconfig.app.json` 无错误。
- `git diff --check`：通过，无 whitespace error。

## Commit

- hash: `d43ce563`
