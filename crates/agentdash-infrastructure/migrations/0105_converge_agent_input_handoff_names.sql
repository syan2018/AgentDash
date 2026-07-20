-- Routine and gate continuation documents retain owner-local receipts for concrete Agent input
-- handoffs. Their schema names describe those receipts directly.

ALTER TABLE routine_executions
    RENAME COLUMN dispatch_mailbox TO dispatch_input_handoff;

UPDATE routine_executions
SET dispatch_input_handoff =
    (dispatch_input_handoff - 'mailbox_message_id')
    || jsonb_build_object('handoff_id', dispatch_input_handoff->'mailbox_message_id')
WHERE dispatch_input_handoff ? 'mailbox_message_id';

ALTER INDEX idx_routine_exec_runtime_operation
    RENAME TO routine_executions_input_operation_id_idx;

ALTER TABLE gate_result_delivery_markers
    RENAME COLUMN mailbox_message_id TO input_handoff_id;

ALTER TABLE gate_result_delivery_markers
    RENAME COLUMN accepted_runtime_operation_id TO accepted_operation_id;
