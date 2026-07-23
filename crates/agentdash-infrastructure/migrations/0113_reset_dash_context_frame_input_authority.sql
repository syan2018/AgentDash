-- Dash owner documents now persist the exact accepted ContextFrame values used for provider
-- input, including surface, initial-context, and compaction frames. Tool definitions also retain
-- their typed provenance so the readable frame and provider-native contract share one identity.
--
-- The project is pre-release and these documents are development execution state rather than
-- Product data. Rebuilding them from newly accepted commands establishes the new single-authority
-- invariant without inventing a second renderer to reconstruct historical prompt text.

DELETE FROM dash_complete_effect;
DELETE FROM dash_complete_source;
