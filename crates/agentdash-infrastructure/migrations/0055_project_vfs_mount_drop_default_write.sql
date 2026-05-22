-- Drop the per-mount default_write flag from project_vfs_mounts.
-- Rationale: a Project VFS Mount should never act as the implicit fs.write target —
-- workspace `main` is the only legitimate default_write surface. Allowing each mount
-- to opt into default_write also produced ambiguous resolution when multiple mounts
-- claimed the flag.

ALTER TABLE project_vfs_mounts DROP COLUMN IF EXISTS default_write;
