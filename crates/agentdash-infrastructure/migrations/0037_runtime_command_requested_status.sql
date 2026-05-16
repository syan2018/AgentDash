UPDATE session_runtime_commands
SET status = 'requested'
WHERE status = 'pending';
