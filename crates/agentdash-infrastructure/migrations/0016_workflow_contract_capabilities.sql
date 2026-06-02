-- 0016: capabilities 从 LifecycleStepDefinition 迁移到 WorkflowContract
--
-- 新模型：
-- - `WorkflowContract.capabilities: Vec<String>` 作为基线能力 key 集合
-- - `LifecycleStepDefinition.capabilities` 字段删除
--
-- 迁移步骤：
-- 1. 扫 `lifecycle_definitions.steps` 内的每个 step
-- 2. 把 step.capabilities 中的 Add 指令 key 汇总到对应 workflow_definitions.contract.capabilities
--    （同一 workflow 被多 step 引用且指令不一致时取并集）
-- 3. 删除 step.capabilities 字段
--
-- 因为 steps / contract 存储为 TEXT（JSON）列，这里用 PL/pgSQL 过程按行处理。

-- ── up: steps.capabilities → workflow.contract.capabilities 合并 ──

DO $$
DECLARE
    lc RECORD;
    step_item JSONB;
    step_caps JSONB;
    directive JSONB;
    wk TEXT;
    add_key TEXT;
    wf_row RECORD;
    wf_contract JSONB;
    existing_caps JSONB;
    merged_caps JSONB;
    new_steps JSONB;
    new_step JSONB;
BEGIN
    FOR lc IN SELECT id, project_id, steps::jsonb AS steps_json FROM lifecycle_definitions LOOP
        IF jsonb_typeof(lc.steps_json) <> 'array' THEN
            CONTINUE;
        END IF;

        new_steps := '[]'::jsonb;

        FOR step_item IN SELECT * FROM jsonb_array_elements(lc.steps_json) LOOP
            step_caps := step_item -> 'capabilities';
            wk := step_item ->> 'workflow_key';

            -- 把 step.capabilities 里的 Add 指令合并到 workflow.contract.capabilities
            IF wk IS NOT NULL AND step_caps IS NOT NULL AND jsonb_typeof(step_caps) = 'array' THEN
                SELECT id, contract::jsonb AS contract_json INTO wf_row
                FROM workflow_definitions
                WHERE project_id = lc.project_id AND key = wk
                LIMIT 1;

                IF FOUND THEN
                    wf_contract := COALESCE(wf_row.contract_json, '{}'::jsonb);
                    existing_caps := COALESCE(wf_contract -> 'capabilities', '[]'::jsonb);

                    FOR directive IN SELECT * FROM jsonb_array_elements(step_caps) LOOP
                        add_key := directive ->> 'add';
                        IF add_key IS NOT NULL THEN
                            -- 并集合并：不重复添加
                            IF NOT (existing_caps @> to_jsonb(add_key)) THEN
                                existing_caps := existing_caps || to_jsonb(add_key);
                            END IF;
                        END IF;
                    END LOOP;

                    merged_caps := existing_caps;
                    wf_contract := jsonb_set(wf_contract, '{capabilities}', merged_caps, true);

                    UPDATE workflow_definitions
                    SET contract = wf_contract::text,
                        updated_at = NOW()::TEXT
                    WHERE id = wf_row.id;

                    RAISE NOTICE 'migrated step %.% → workflow % capabilities=%',
                        lc.id, step_item ->> 'key', wk, merged_caps;
                ELSE
                    RAISE WARNING 'lifecycle % step % 引用的 workflow_key=% 不存在,跳过 capability 合并',
                        lc.id, step_item ->> 'key', wk;
                END IF;
            END IF;

            -- 无论是否成功合并,一律从 step 上剥离 capabilities 字段
            new_step := step_item - 'capabilities';
            new_steps := new_steps || new_step;
        END LOOP;

        UPDATE lifecycle_definitions
        SET steps = new_steps::text,
            updated_at = NOW()::TEXT
        WHERE id = lc.id;
    END LOOP;
END $$;
