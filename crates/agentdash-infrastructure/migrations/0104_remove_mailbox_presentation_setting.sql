-- Conversation presentation now follows concrete Agent state and LifecycleGate waits. There is no
-- mailbox projection whose system-generated rows require a user visibility preference.

DELETE FROM settings
WHERE key = 'agent.mailbox.hide_system_steer_messages';
