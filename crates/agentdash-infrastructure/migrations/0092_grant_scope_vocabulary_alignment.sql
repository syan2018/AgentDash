-- GrantScope 词汇对齐：session → agent_frame，workflow_step → activity
UPDATE permission_grants
SET grant_scope = 'agent_frame'
WHERE grant_scope = 'session';

UPDATE permission_grants
SET grant_scope = 'activity'
WHERE grant_scope = 'workflow_step';
