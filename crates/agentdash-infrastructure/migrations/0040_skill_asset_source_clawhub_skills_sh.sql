ALTER TABLE skill_assets
DROP CONSTRAINT IF EXISTS skill_assets_source_check;

ALTER TABLE skill_assets
ADD CONSTRAINT skill_assets_source_check
CHECK (source IN ('builtin_seed', 'user', 'github', 'clawhub', 'skills_sh'));
