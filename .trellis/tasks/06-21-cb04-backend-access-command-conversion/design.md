# CB04-F Design

## Boundary

- Backend response DTO projection may remain contract-owned if it is narrow outbound mapping.
- Backend access command/status parsing belongs to API adapter/application command boundary.

## Execution Shape

- Start with owner review.
- Implement migration only for reverse conversions that are request-command parsing.
