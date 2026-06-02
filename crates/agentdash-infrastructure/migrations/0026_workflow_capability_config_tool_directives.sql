-- 0026: workflow contract 工具能力指令迁移到 capability_config.tool_directives
--
-- 当前正式契约：
--   WorkflowContract.capability_config.tool_directives: Vec<ToolCapabilityDirective>
--
-- 迁移目标：
-- - contract.capability_directives -> contract.capability_config.tool_directives
-- - contract.capabilities -> Add(simple) tool_directives（防御历史库未完成 0018 的情况）
-- - 删除 contract 根部的 capability_directives / capabilities 旧字段

DO $$
DECLARE
    wf_row RECORD;
    wf_contract JSONB;
    capability_config JSONB;
    existing_tool_directives JSONB;
    legacy_directives JSONB;
    legacy_capabilities JSONB;
    capability_item JSONB;
    capability_key TEXT;
    capabilities_as_directives JSONB;
    merged_tool_directives JSONB;
    migrated_count INTEGER := 0;
BEGIN
    FOR wf_row IN SELECT id, contract::jsonb AS contract_json FROM workflow_definitions LOOP
        wf_contract := COALESCE(wf_row.contract_json, '{}'::jsonb);

        IF NOT (wf_contract ? 'capability_directives')
            AND NOT (wf_contract ? 'capabilities')
        THEN
            CONTINUE;
        END IF;

        capability_config := COALESCE(wf_contract -> 'capability_config', '{}'::jsonb);

        existing_tool_directives := CASE
            WHEN jsonb_typeof(capability_config -> 'tool_directives') = 'array'
            THEN capability_config -> 'tool_directives'
            ELSE '[]'::jsonb
        END;

        legacy_directives := CASE
            WHEN jsonb_typeof(wf_contract -> 'capability_directives') = 'array'
            THEN wf_contract -> 'capability_directives'
            ELSE '[]'::jsonb
        END;

        legacy_capabilities := CASE
            WHEN jsonb_typeof(wf_contract -> 'capabilities') = 'array'
            THEN wf_contract -> 'capabilities'
            ELSE '[]'::jsonb
        END;

        capabilities_as_directives := '[]'::jsonb;
        FOR capability_item IN SELECT * FROM jsonb_array_elements(legacy_capabilities) LOOP
            IF jsonb_typeof(capability_item) = 'string' THEN
                capability_key := capability_item #>> '{}';
                IF capability_key IS NOT NULL AND length(trim(capability_key)) > 0 THEN
                    capabilities_as_directives :=
                        capabilities_as_directives ||
                        jsonb_build_array(jsonb_build_object('add', capability_key));
                END IF;
            END IF;
        END LOOP;

        -- 若新旧字段同时存在，让新字段排在后面，保持“显式新配置覆盖历史根字段”的顺序语义。
        merged_tool_directives :=
            legacy_directives || capabilities_as_directives || existing_tool_directives;

        IF jsonb_array_length(merged_tool_directives) > 0 THEN
            capability_config :=
                jsonb_set(capability_config, '{tool_directives}', merged_tool_directives, true);
            wf_contract :=
                jsonb_set(wf_contract, '{capability_config}', capability_config, true);
        ELSIF capability_config = '{}'::jsonb THEN
            wf_contract := wf_contract - 'capability_config';
        ELSE
            wf_contract :=
                jsonb_set(wf_contract, '{capability_config}', capability_config, true);
        END IF;

        wf_contract := wf_contract - 'capability_directives' - 'capabilities';

        UPDATE workflow_definitions
        SET contract = wf_contract::text,
            updated_at = NOW()
        WHERE id = wf_row.id;

        migrated_count := migrated_count + 1;
    END LOOP;

    RAISE NOTICE 'workflow capability_config.tool_directives 迁移完成，更新 workflow 数量=%',
        migrated_count;
END $$;
