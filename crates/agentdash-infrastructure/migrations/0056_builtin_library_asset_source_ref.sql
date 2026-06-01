UPDATE agent_procedures p
SET
    source_ref = COALESCE(source_ref, key),
    source_version = COALESCE(source_version, version::text)
WHERE source_ref IS NULL;

UPDATE workflow_graphs g
SET
    source_ref = COALESCE(source_ref, key),
    source_version = COALESCE(source_version, version::text)
WHERE source_ref IS NULL;
