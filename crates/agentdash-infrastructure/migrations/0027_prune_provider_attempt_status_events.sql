-- Provider attempt status is a live connection/retry signal. Durable session history keeps
-- terminal/error/rewind facts, so previously persisted provider status rows can be pruned.
DELETE FROM session_events
WHERE session_update_type = 'provider_attempt_status';
