# CB04-C Design

## Boundary

- Application owns context usage analysis because it interprets SPI `ContextFrame`.
- Contract owns response DTO shape only.
- Mapping from application usage facts to DTO happens at stream/API boundary.

## Conflict Boundary

- This task owns context usage projection only.
- It must not alter session terminal/lost semantics or AgentRun mailbox behavior.
