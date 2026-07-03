ALTER TABLE IF EXISTS agent_run_command_receipts
    DROP CONSTRAINT IF EXISTS agent_run_command_receipts_command_kind_check;

ALTER TABLE IF EXISTS agent_run_command_receipts
    ADD CONSTRAINT agent_run_command_receipts_command_kind_check CHECK (
        command_kind = ANY (ARRAY[
            'message_submit'::text,
            'project_agent_start'::text,
            'agent_run_fork'::text,
            'agent_run_fork_submit'::text,
            'mailbox_promote'::text,
            'mailbox_delete'::text,
            'mailbox_resume'::text,
            'cancel'::text
        ])
    );
