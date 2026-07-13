-- Runtime persistence now stores the complete RuntimeJournalRecord carrier and fact union.
-- Existing pre-release rows contain the narrower RuntimeEventEnvelope shape and cannot be
-- upgraded without inventing presentation payloads, so the Runtime owner graph is reprovisioned.

DELETE FROM agent_run_runtime_binding_lineage;
DELETE FROM agent_run_runtime_recovery_intent;
DELETE FROM agent_run_runtime_thread_anchor;
UPDATE agent_run_mailbox_messages
SET accepted_runtime_operation_id = NULL
WHERE accepted_runtime_operation_id IS NOT NULL;
DELETE FROM permission_grants
WHERE source_runtime_operation_id IS NOT NULL;
DELETE FROM agent_runtime_thread;
DELETE FROM agent_runtime_binding;

ALTER TABLE agent_runtime_event
    RENAME COLUMN event_kind TO fact_kind;

ALTER TABLE agent_runtime_event
    RENAME COLUMN envelope TO record;

ALTER TABLE agent_runtime_event
    ADD CONSTRAINT agent_runtime_event_fact_kind_check
    CHECK (
        fact_kind IN ('presentation', 'internal')
        AND record -> 'fact' ->> 'kind' IS NOT NULL
        AND fact_kind = record -> 'fact' ->> 'kind'
    );
