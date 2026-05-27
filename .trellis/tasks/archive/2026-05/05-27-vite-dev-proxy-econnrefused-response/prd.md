# Vite Dev Proxy ECONNREFUSED 响应修正

## Goal

修正 `packages/app-web` dev proxy 在后端未启动时对 API 请求静默悬挂的问题。

## Requirements

- `ECONNRESET` / `EPIPE` 这类客户端断开噪音可继续静默。
- `ECONNREFUSED` 必须返回明确的开发态 502 响应。
- 不改变生产 API 行为。

## Acceptance Criteria

- [ ] Vite proxy error handler 对 `ECONNREFUSED` 调用 `res.end`。
- [ ] `pnpm --filter app-web typecheck` 或等价前端检查通过。
