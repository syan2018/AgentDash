-- 0017: LifecycleEdge 分离 flow / artifact 两种 kind，并补齐历史 lifecycle 的 edges
--
-- 新模型：
-- - `LifecycleEdge.kind: "flow" | "artifact"`
--   - Flow：仅承载顺序约束，无 port 字段
--   - Artifact：端口级数据依赖，必须声明 from_port/to_port；自动蕴含 flow 约束
-- - Runtime 的 fallback 线性推进已移除，所有 lifecycle 必须显式声明 edges
--
-- 迁移步骤：
-- 1. 对既有 edges：为每条 edge 补上 "kind": "artifact" 字段（历史 edge 全部是 port-based）
-- 2. 对 steps.len() >= 2 且 edges 为空的 lifecycle：按 steps 数组顺序补线性 flow edges
--    （这是原 runtime fallback 的数据化表达）
-- 3. 单 step lifecycle 无需处理（终态由无出边天然决定）

-- ── up: 补齐 kind 字段 + 历史 fallback lifecycle 补线性 flow edges ──

DO $$
DECLARE
    lc RECORD;
    steps_json JSONB;
    edges_json JSONB;
    new_edges JSONB;
    edge_item JSONB;
    patched_edge JSONB;
    step_count INT;
    i INT;
    from_key TEXT;
    to_key TEXT;
BEGIN
    FOR lc IN SELECT id, steps::jsonb AS steps_json, edges::jsonb AS edges_json
              FROM lifecycle_definitions LOOP
        steps_json := lc.steps_json;
        edges_json := COALESCE(lc.edges_json, '[]'::jsonb);

        IF jsonb_typeof(steps_json) <> 'array' THEN
            CONTINUE;
        END IF;

        step_count := jsonb_array_length(steps_json);

        IF jsonb_array_length(edges_json) > 0 THEN
            -- Path A: 既有 edges 补 kind="artifact"（如缺失）
            new_edges := '[]'::jsonb;
            FOR edge_item IN SELECT * FROM jsonb_array_elements(edges_json) LOOP
                IF edge_item ? 'kind' THEN
                    patched_edge := edge_item;
                ELSE
                    patched_edge := jsonb_set(edge_item, '{kind}', '"artifact"'::jsonb, true);
                END IF;
                new_edges := new_edges || patched_edge;
            END LOOP;

            UPDATE lifecycle_definitions
            SET edges = new_edges::text,
                updated_at = NOW()::TEXT
            WHERE id = lc.id;

            RAISE NOTICE 'lifecycle % edges 补齐 kind 字段 (count=%)', lc.id, jsonb_array_length(new_edges);

        ELSIF step_count >= 2 THEN
            -- Path B: 历史 fallback lifecycle → 按 steps 数组顺序补线性 flow edges
            new_edges := '[]'::jsonb;
            FOR i IN 0..(step_count - 2) LOOP
                from_key := steps_json -> i ->> 'key';
                to_key := steps_json -> (i + 1) ->> 'key';
                IF from_key IS NULL OR to_key IS NULL THEN
                    RAISE WARNING 'lifecycle % step[%]/step[%] 缺少 key 字段，跳过 edge 补齐',
                        lc.id, i, i + 1;
                    CONTINUE;
                END IF;
                new_edges := new_edges || jsonb_build_object(
                    'kind', 'flow',
                    'from_node', from_key,
                    'to_node', to_key
                );
            END LOOP;

            UPDATE lifecycle_definitions
            SET edges = new_edges::text,
                updated_at = NOW()::TEXT
            WHERE id = lc.id;

            RAISE NOTICE 'lifecycle % 补线性 flow edges (steps=%, edges=%)',
                lc.id, step_count, jsonb_array_length(new_edges);
        END IF;
        -- 单 step lifecycle: 无需处理
    END LOOP;
END $$;
