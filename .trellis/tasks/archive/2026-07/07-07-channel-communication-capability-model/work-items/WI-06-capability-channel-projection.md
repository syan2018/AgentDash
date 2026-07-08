# WI-06 CapabilityState.channel Projection

Status: done
Owner: implement worker
Depends On: WI-01, WI-04
Can Run With: WI-03, WI-05 partial
Expected Commit: `feat(capability): 增加 channel capability 投影`

## Scope

- Add `CapabilityState.channel` default empty dimension.
- Register channel dimension with `AccumulationPolicy::Accumulate`.
- Define `ChannelDirective::Expose/Revoke` typed payload validation.
- Implement `ChannelCapabilityProjector`.
- Keep tool visibility separate from channel operation admission.

## Exit Criteria

- Old frame JSON deserializes with empty channel state.
- Expose/Revoke replay works.
- Projection does not write registry facts.

## Targeted Checks

```powershell
cargo test -p agentdash-spi channel
cargo test -p agentdash-application-agentrun channel
cargo check -p agentdash-spi -p agentdash-application-agentrun
```

## Progress Log

- initialized
- candidate implementation exists in workspace for SPI dimension, runtime replay and projection tests
- implemented `CapabilityState.channel`, channel delta reporting, channel Accumulate effect constants, `ChannelCapabilityDimensionModule` Expose/Revoke replay and normalization
- targeted checks were run by host and must be verified by native check worker before this item can move forward: `cargo test -p agentdash-spi channel`; `cargo test -p agentdash-application-agentrun channel`; `cargo check -p agentdash-spi -p agentdash-application-agentrun`
- native check worker `Ohm` completed WI-10 full-scope check; channel projection verification passed
- dispatcher integration review passed; affected-package cargo check passed
