-- 0018: WorkflowContract.capabilities → capability_directives（hard cutover）
--
-- 新模型：
-- - `WorkflowContract.capability_directives: Vec<CapabilityDirective>`
-- - 每条 Directive 形如 `{"add": "<qualified_path>"}` / `{"remove": "<qualified_path>"}`
-- - `CapabilityDetailedEntry` / `CapabilityEntry` 类型整体删除；serde 不再兼容老 `capabilities` key
--
-- 迁移规则（一次性改写 workflow_definitions.contract JSON）：
-- - 老 entry 为纯字符串 `"file_read"` → `[{"add":"file_read"}]`
-- - 老 entry 为 `{"key":"file_read","exclude_tools":["fs_grep"]}`
--   → `[{"add":"file_read"},{"remove":"file_read::fs_grep"}]`
-- - 老 entry 为 `{"key":"file_read","include_tools":["fs_read"]}`
--   → `[{"add":"file_read::fs_read"}]`
-- - 老别名 `"file_system"` → 拆为 `[{"add":"file_read"},{"add":"file_write"},{"add":"shell_execute"}]`
-- - 同时声明 include_tools 和 exclude_tools 的非法组合：RAISE WARNING 后跳过（业务侧必须先修正数据）
--
-- 幂等性：`contract` 中已包含 `capability_directives` 键时直接跳过；不会重复产出。
--
-- 因为 contract 存储为 TEXT（JSON）列，整体 PL/pgSQL 按行处理。

DO $$
DECLARE
    wf RECORD;
    wf_contract JSONB;
    legacy_caps JSONB;
    new_directives JSONB;
    cap_item JSONB;
    cap_key TEXT;
    include_arr JSONB;
    exclude_arr JSONB;
    tool_item JSONB;
BEGIN
    FOR wf IN SELECT id, contract::jsonb AS contract_json FROM workflow_definitions LOOP
        wf_contract := COALESCE(wf.contract_json, '{}'::jsonb);

        -- 幂等保护：已迁移过的行直接跳过
        IF wf_contract ? 'capability_directives' THEN
            CONTINUE;
        END IF;

        legacy_caps := wf_contract -> 'capabilities';

        -- 老字段不存在或不是数组时：插入空 directives 并清理 capabilities 键
        IF legacy_caps IS NULL OR jsonb_typeof(legacy_caps) <> 'array' THEN
            wf_contract := wf_contract - 'capabilities';
            UPDATE workflow_definitions
            SET contract = wf_contract::text,
                updated_at = NOW()::TEXT
            WHERE id = wf.id;
            CONTINUE;
        END IF;

        new_directives := '[]'::jsonb;

        FOR cap_item IN SELECT * FROM jsonb_array_elements(legacy_caps) LOOP
            -- 形态 1：纯字符串 key
            IF jsonb_typeof(cap_item) = 'string' THEN
                cap_key := cap_item #>> '{}';
                IF cap_key = 'file_system' THEN
                    -- 别名展开
                    new_directives := new_directives
                        || jsonb_build_object('add', 'file_read')
                        || jsonb_build_object('add', 'file_write')
                        || jsonb_build_object('add', 'shell_execute');
                ELSE
                    new_directives := new_directives
                        || jsonb_build_object('add', cap_key);
                END IF;
                CONTINUE;
            END IF;

            -- 形态 2：结构化条目 {key, include_tools?, exclude_tools?}
            IF jsonb_typeof(cap_item) = 'object' THEN
                cap_key := cap_item ->> 'key';
                IF cap_key IS NULL OR cap_key = '' THEN
                    RAISE WARNING 'workflow % 存在 capabilities 条目缺少 key 字段，跳过: %',
                        wf.id, cap_item;
                    CONTINUE;
                END IF;

                include_arr := cap_item -> 'include_tools';
                exclude_arr := cap_item -> 'exclude_tools';

                -- 非法组合：同时声明 include 和 exclude
                IF include_arr IS NOT NULL
                   AND jsonb_typeof(include_arr) = 'array'
                   AND jsonb_array_length(include_arr) > 0
                   AND exclude_arr IS NOT NULL
                   AND jsonb_typeof(exclude_arr) = 'array'
                   AND jsonb_array_length(exclude_arr) > 0 THEN
                    RAISE WARNING 'workflow % capability % 同时声明 include_tools 和 exclude_tools，迁移跳过（需人工修正）',
                        wf.id, cap_key;
                    CONTINUE;
                END IF;

                -- include_tools 非空：展开为工具级 Add
                IF include_arr IS NOT NULL
                   AND jsonb_typeof(include_arr) = 'array'
                   AND jsonb_array_length(include_arr) > 0 THEN
                    FOR tool_item IN SELECT * FROM jsonb_array_elements(include_arr) LOOP
                        new_directives := new_directives
                            || jsonb_build_object(
                                'add',
                                cap_key || '::' || (tool_item #>> '{}')
                            );
                    END LOOP;
                    CONTINUE;
                END IF;

                -- 其余：先 Add 能力级，exclude_tools 展开为工具级 Remove
                IF cap_key = 'file_system' THEN
                    -- 别名 + exclude_tools 的复合：别名先拆开，exclude 逐个落到三能力上
                    new_directives := new_directives
                        || jsonb_build_object('add', 'file_read')
                        || jsonb_build_object('add', 'file_write')
                        || jsonb_build_object('add', 'shell_execute');
                ELSE
                    new_directives := new_directives
                        || jsonb_build_object('add', cap_key);
                END IF;

                IF exclude_arr IS NOT NULL
                   AND jsonb_typeof(exclude_arr) = 'array' THEN
                    FOR tool_item IN SELECT * FROM jsonb_array_elements(exclude_arr) LOOP
                        -- 对 file_system 别名：exclude 的工具无法精确归属三能力之一，
                        -- 统一附在 file_read 作为 path（业务侧新结构应直接使用细粒度 key，
                        -- 若必要再人工修正）
                        IF cap_key = 'file_system' THEN
                            new_directives := new_directives
                                || jsonb_build_object(
                                    'remove',
                                    'file_read::' || (tool_item #>> '{}')
                                );
                        ELSE
                            new_directives := new_directives
                                || jsonb_build_object(
                                    'remove',
                                    cap_key || '::' || (tool_item #>> '{}')
                                );
                        END IF;
                    END LOOP;
                END IF;
            END IF;
        END LOOP;

        -- 写回新字段并删除老字段
        wf_contract := jsonb_set(wf_contract, '{capability_directives}', new_directives, true);
        wf_contract := wf_contract - 'capabilities';

        UPDATE workflow_definitions
        SET contract = wf_contract::text,
            updated_at = NOW()::TEXT
        WHERE id = wf.id;

        RAISE NOTICE 'workflow % capabilities → capability_directives 迁移完成 (directives=%)',
            wf.id, jsonb_array_length(new_directives);
    END LOOP;
END $$;
