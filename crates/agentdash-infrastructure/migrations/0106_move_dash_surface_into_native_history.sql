-- A materialized Dash surface is a concrete Agent fact. Move the current surface from the
-- repository side field into the source's ordered history so execution, reconnect presentation,
-- fork and ContextFrame projection recover from one owner-local fact chain.

WITH candidates AS (
    SELECT
        source.source_coordinate,
        source.document,
        source.document #> '{repository,surface}' AS surface,
        COALESCE(source.document #> '{repository,store,history,entries}', '[]'::jsonb) AS entries,
        COALESCE(source.document #> '{repository,store,changes}', '[]'::jsonb) AS changes
    FROM dash_complete_source AS source
    WHERE jsonb_typeof(source.document #> '{repository,surface}') = 'object'
      AND COALESCE(
            source.document #>> '{repository,store,changes,-1,state,status}',
            'open'
          ) = 'open'
      AND NOT EXISTS (
            SELECT 1
            FROM jsonb_array_elements(
                COALESCE(source.document #> '{repository,store,history,entries}', '[]'::jsonb)
            ) AS history_entry
            WHERE history_entry #>> '{payload,type}' = 'surface_applied'
          )
), appended AS (
    SELECT
        candidate.*,
        jsonb_array_length(candidate.entries) + 1 AS next_sequence,
        format(
            'surface-applied:migration:%s:%s',
            candidate.surface ->> 'revision',
            candidate.surface ->> 'digest'
        ) AS entry_id,
        jsonb_build_object(
            'entry_id', format(
                'surface-applied:migration:%s:%s',
                candidate.surface ->> 'revision',
                candidate.surface ->> 'digest'
            ),
            'sequence', jsonb_array_length(candidate.entries) + 1,
            'parent_entry_id', candidate.entries #> ARRAY[
                (jsonb_array_length(candidate.entries) - 1)::text,
                'entry_id'
            ],
            'payload', jsonb_build_object(
                'type', 'surface_applied',
                'surface', candidate.surface
            )
        ) AS entry
    FROM candidates AS candidate
), folded AS (
    SELECT
        appended.*,
        jsonb_set(
            jsonb_set(
                jsonb_set(
                    COALESCE(
                        appended.changes #> ARRAY[
                            (jsonb_array_length(appended.changes) - 1)::text,
                            'state'
                        ],
                        jsonb_build_object(
                            'session_id', appended.document #>> '{repository,store,history,session_id}',
                            'branch_id', appended.document #>> '{repository,store,history,branch_id}',
                            'head', NULL,
                            'entry_count', 0,
                            'status', 'open',
                            'initial_context', NULL,
                            'surface', NULL,
                            'accepted_inputs', jsonb_build_array(),
                            'active_turn', NULL,
                            'active_compaction', NULL,
                            'turns', jsonb_build_object(),
                            'items', jsonb_build_object(),
                            'interactions', jsonb_build_object(),
                            'compactions', jsonb_build_object()
                        )
                    ),
                    '{head}',
                    to_jsonb(appended.entry_id)
                ),
                '{entry_count}',
                to_jsonb(appended.next_sequence)
            ),
            '{surface}',
            appended.surface
        ) AS state
    FROM appended
), replacement AS (
    SELECT
        folded.source_coordinate,
        jsonb_set(
            jsonb_set(
                jsonb_set(
                    folded.document #- '{repository,surface}',
                    '{repository,store,history,entries}',
                    folded.entries || jsonb_build_array(folded.entry)
                ),
                '{repository,store,changes}',
                folded.changes || jsonb_build_array(jsonb_build_object(
                    'cursor', jsonb_build_object(
                        'revision', folded.next_sequence,
                        'ordinal', 0
                    ),
                    'head', folded.entry_id,
                    'source_digest', format(
                        'surface-migration:%s',
                        folded.surface ->> 'digest'
                    ),
                    'state', folded.state,
                    'payload', jsonb_build_object(
                        'type', 'history_entry',
                        'entry', folded.entry
                    )
                ))
            ),
            '{metadata}',
            folded.document -> 'metadata'
        ) AS document
    FROM folded
)
UPDATE dash_complete_source AS source
SET document = replacement.document
FROM replacement
WHERE source.source_coordinate = replacement.source_coordinate;

-- Closed sources cannot accept new history after their terminal entry. Their materialized surface
-- is no longer executable, so clear both the retired repository field and live materialization
-- evidence instead of inventing a post-close history mutation.
UPDATE dash_complete_source AS source
SET document = jsonb_set(
    jsonb_set(
        jsonb_set(
            jsonb_set(
                source.document #- '{repository,surface}',
                '{metadata,applied_surface}',
                'null'::jsonb
            ),
            '{metadata,callback_surface}',
            'null'::jsonb
        ),
        '{metadata,callback_binding}',
        'null'::jsonb
    ),
    '{metadata,initial_context}',
    COALESCE(source.document #> '{metadata,initial_context}', 'null'::jsonb)
)
WHERE jsonb_typeof(source.document #> '{repository,surface}') = 'object';
