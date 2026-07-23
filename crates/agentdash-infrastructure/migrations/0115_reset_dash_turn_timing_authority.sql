-- Turn timing is now an explicit part of the native Dash history contract so completed turns
-- retain their start time and duration across snapshot reloads. These pre-release rows are
-- development execution state; rebuilding them from new commands establishes the authoritative
-- timing shape without maintaining a second legacy history decoder.

DELETE FROM dash_complete_effect;
DELETE FROM dash_complete_source;
