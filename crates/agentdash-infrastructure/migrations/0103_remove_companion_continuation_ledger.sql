-- Companion dispatch is a synchronous Product handoff. Its downstream Product and Agent effects
-- already own stable identities and durable receipts, so the dispatcher itself has no independent
-- cross-restart fact to persist.

DROP TABLE companion_continuation_saga;
