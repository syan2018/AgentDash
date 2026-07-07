CREATE TABLE IF NOT EXISTS gate_result_delivery_markers (
    gate_id text NOT NULL REFERENCES lifecycle_gates(id) ON DELETE CASCADE,
    result_attempt integer NOT NULL,
    status text NOT NULL,
    target_run_id text,
    target_agent_id text,
    target_waiter_ref text,
    mailbox_message_id text,
    command_receipt_id text,
    claim_token text,
    claim_expires_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (gate_id, result_attempt),
    CONSTRAINT gate_result_delivery_markers_status_check CHECK (
        status IN (
            'pending',
            'delivered_to_waiter',
            'queued_for_parent_continuation',
            'dispatched_to_parent'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_gate_result_delivery_markers_status
    ON gate_result_delivery_markers(status, claim_expires_at);

CREATE INDEX IF NOT EXISTS idx_gate_result_delivery_markers_target
    ON gate_result_delivery_markers(target_run_id, target_agent_id);
