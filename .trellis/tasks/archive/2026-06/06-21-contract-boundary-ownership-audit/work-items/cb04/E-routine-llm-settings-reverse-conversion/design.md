# CB04-E Design

## Boundary

- Contract DTO owns request/response wire shape.
- Route/application command mapper owns DTO -> domain value parsing.
- Outbound projections may remain in contracts when they are narrow domain -> DTO mappings.

## Conflict Boundary

- This task owns Routine / LLM provider / Settings reverse conversion only.
- It must not edit MCP preset conversion or backend access conversion.
