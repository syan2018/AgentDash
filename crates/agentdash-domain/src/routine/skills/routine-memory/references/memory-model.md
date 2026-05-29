# Routine Memory Model

Routine memory is stored under the `routine://` mount.

## Current Trigger

`current/trigger.json` contains the trigger source, payload, routine id, execution id, and optional entity key.

`current/execution.json` contains the current `RoutineExecution` projection.

`current/resolved-prompt.md` contains the prompt after template rendering.

These files are read-only.

## Routine-Level Memory

`memory/brief.md` describes the Routine's durable purpose and operating assumptions.

`memory/facts.md` records durable facts that future triggers should know.

`memory/decisions.md` records decisions that should steer future work.

`memory/open-items.md` records unresolved follow-up work.

`memory/changelog.md` records notable memory changes when useful.

## Entity-Level Memory

`entities/{entity_key}/brief.md` summarizes the external entity tracked by this Routine.

`entities/{entity_key}/facts.md` records durable facts about that entity.

`entities/{entity_key}/open-items.md` records unresolved work for that entity.

`entities/{entity_key}/last-run.md` summarizes the latest meaningful outcome for that entity.

Only the current entity is writable during a Routine session.
