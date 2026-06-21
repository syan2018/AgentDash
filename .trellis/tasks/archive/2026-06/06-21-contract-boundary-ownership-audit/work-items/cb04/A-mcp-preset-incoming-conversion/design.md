# CB04-A Design

## Boundary

- Contract DTO owns wire fields and outbound projection only.
- API adapter/application command builder owns incoming DTO parsing into domain transport config, runtime binding, binding target/source and route policy.
- Mapping code may live in a route-local mapper module if it stays adapter-owned.

## Conflict Boundary

- This task owns MCP preset conversion surfaces.
- It must not edit AgentRun workspace snapshot, session context usage projection, capability catalog, routine/LLM/settings or backend access conversion.

## Validation Shape

- Unit tests should assert incoming request mapping semantics at adapter/application boundary.
- Contract tests should assert DTO serde / TypeScript generation still succeeds.
