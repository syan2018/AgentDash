UPDATE public.dash_complete_source
SET repository = jsonb_set(repository, '{active,lease}', 'null'::jsonb, true)
WHERE jsonb_typeof(repository->'active') = 'object'
  AND NOT ((repository->'active') ? 'lease');
