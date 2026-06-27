# CB04-B Design

## Boundary

- Application read model owns workspace shell, command availability core facts, execution state, resource surface facts and subject association facts.
- API adapter owns final contract DTO construction.
- Generated DTOs remain route output / frontend contract only.

## Dependency

- Blocked by Runtime Coordinate RC02 because workspace snapshot must consume unified delivery selection, not raw anchor latest.

## Conflict Boundary

- This task owns AgentRun workspace/conversation snapshot split.
- It should not run in parallel with Runtime Coordinate consumer migrations.
