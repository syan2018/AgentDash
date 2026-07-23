-- Repository history and Complete Agent metadata are independently read projections of the
-- same source owner. Keeping them in distinct typed JSONB columns lets ordinary reads fetch only
-- the authority they need, while write transactions still update both columns atomically.
--
-- Development execution state was reset by the preceding context-frame authority migration, so
-- this pre-release schema convergence establishes the new shape without a legacy decoder.

DELETE FROM dash_complete_effect;
DELETE FROM dash_complete_source;

ALTER TABLE dash_complete_source
    DROP CONSTRAINT dash_complete_source_document_shape,
    DROP CONSTRAINT dash_complete_source_document_object,
    DROP COLUMN document,
    ADD COLUMN repository JSONB NOT NULL,
    ADD COLUMN metadata JSONB NOT NULL,
    ADD COLUMN observation JSONB NOT NULL,
    ADD CONSTRAINT dash_complete_source_repository_object
        CHECK (jsonb_typeof(repository) = 'object'),
    ADD CONSTRAINT dash_complete_source_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object'),
    ADD CONSTRAINT dash_complete_source_observation_object
        CHECK (jsonb_typeof(observation) = 'object');
