# Contract Boundary Owner Map

## Owner Rules

- Application read model owns backend-internal query state, use case facts, policy preconditions and aggregation decisions before they become browser-facing JSON.
- API adapter owns route request parsing into application command/read inputs and application read model to contract DTO mapping.
- Contract DTO owns serde wire shape, generated TypeScript source, HTTP/NDJSON envelope shape and intentionally shared protocol value objects.
- Allowed projection conversion is narrow, outward-only mapping from stable domain/SPI/protocol facts into DTO fields.

## Primary Migration Candidates

| Area | Action | Reason |
| --- | --- | --- |
| MCP preset incoming conversion | split task | DTO -> domain transport/runtime binding/route policy mapping carries command semantics. |
| AgentRun workspace snapshot | split task | application query/policy currently uses generated workflow DTOs as internal read model. |
| Session context usage helper | split task | contract crate currently analyzes SPI `ContextFrame`; this belongs to application projection. |
| Capability catalog | split task | application service returns generated catalog DTO instead of application read model. |
| Routine / LLM / Settings reverse conversion | migrate | request DTO -> domain value parsing should live near route/application command handlers. |
| Backend access command conversion | review then migrate | response projection can stay, but command/status parsing should move to adapter/application boundary. |

## Full Audit

Detailed module-by-module mapping and CB04 candidate descriptions live in:

- `research/01-ownership-audit.md`
- `research/cb03-owner-map.md`

