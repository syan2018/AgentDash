# CB04-B AgentRun workspace snapshot read model split

## Goal

将 AgentRun workspace snapshot 拆为 application read model 与 API adapter DTO mapper。

## Requirements

- Application workspace query / command policy 不以 browser-facing generated DTO 作为内部 read model。
- API adapter 负责 application read model -> AgentRun workspace / conversation snapshot contract DTO mapping。
- 本任务等待 Runtime Coordinate 的 current delivery binding / selection service 可用后再实现。

## Acceptance Criteria

- [ ] application workspace query 返回 backend-owned read model。
- [ ] command policy 不依赖 generated conversation snapshot DTO 表达业务判断。
- [ ] API route adapter 显式映射到 browser-facing DTO。
- [ ] resource surface source coordinate 与 Runtime Coordinate selection 语义一致。

## Notes

- Do not dispatch before Runtime Coordinate RC02 lands.
