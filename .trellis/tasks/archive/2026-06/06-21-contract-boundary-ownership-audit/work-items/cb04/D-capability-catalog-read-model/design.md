# CB04-D Design

## Boundary

- Application read model owns capability catalog facts, including tool identity, source and scope facts.
- API adapter owns browser-facing `CapabilityCatalogResponse` mapping.
- Contracts remain DTO/generation owner.

## Conflict Boundary

- This task owns capability catalog service and API mapping.
- It should not modify AgentFrame exposure fact model; that belongs to Capability Exposure task.
