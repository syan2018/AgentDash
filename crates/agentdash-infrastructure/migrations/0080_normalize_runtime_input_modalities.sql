UPDATE agent_runtime_offer
SET offer = replace(offer::text, '"file_reference"', '"resource"')::jsonb,
    updated_at = now()
WHERE offer::text LIKE '%"file_reference"%';

UPDATE agent_runtime_service_activation
SET effective_profile =
        replace(effective_profile::text, '"file_reference"', '"resource"')::jsonb
WHERE effective_profile::text LIKE '%"file_reference"%';
