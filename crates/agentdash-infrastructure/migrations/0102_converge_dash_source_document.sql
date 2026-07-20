-- A Dash source owns one durable document. Repository history and Complete Agent materialization
-- metadata change atomically because both describe the same concrete Agent source.

ALTER TABLE dash_complete_source
    ADD COLUMN document JSONB;

UPDATE dash_complete_source AS source
SET document = jsonb_build_object(
    'repository', session.repository,
    'metadata', source.metadata
)
FROM dash_agent_session AS session
WHERE session.source_coordinate = source.source_coordinate;

ALTER TABLE dash_complete_source
    ALTER COLUMN document SET NOT NULL,
    ADD CONSTRAINT dash_complete_source_document_object
        CHECK (jsonb_typeof(document) = 'object'),
    ADD CONSTRAINT dash_complete_source_document_shape
        CHECK (
            jsonb_typeof(document -> 'repository') = 'object'
            AND jsonb_typeof(document -> 'metadata') = 'object'
        ),
    DROP CONSTRAINT dash_complete_source_source_coordinate_fkey,
    DROP COLUMN metadata;

DROP TABLE dash_agent_session;
