UPDATE interaction_presentation_states
SET presentation_key = 'canvas:renderer-observation'
WHERE presentation_key = 'canvas.renderer-observation';

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM interaction_presentation_states
        WHERE presentation_key = 'canvas.renderer-observation'
    ) THEN
        RAISE EXCEPTION
            'legacy Canvas renderer presentation keys remain after migration';
    END IF;
END
$$;
