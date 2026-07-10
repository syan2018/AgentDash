use super::value_objects::*;

pub fn validate_agent_procedure(
    key: &str,
    name: &str,
    contract: &AgentProcedureContract,
) -> Result<(), String> {
    validate_identity("procedure.key", key)?;
    validate_non_empty("procedure.name", name)?;
    validate_contract(contract, "procedure.contract")
}

pub fn validate_workflow_graph(
    key: &str,
    name: &str,
    entry_activity_key: &str,
    activities: &[ActivityDefinition],
    transitions: &[ActivityTransition],
) -> Result<(), String> {
    validate_identity("lifecycle.key", key)?;
    validate_non_empty("lifecycle.name", name)?;
    validate_identity("lifecycle.entry_activity_key", entry_activity_key)?;
    if activities.is_empty() {
        return Err("lifecycle.activities 至少需要一个 activity".to_string());
    }

    let mut seen_activity_keys = std::collections::BTreeSet::new();
    for (index, activity) in activities.iter().enumerate() {
        let path = format!("lifecycle.activities[{index}]");
        validate_identity(&format!("{path}.key"), &activity.key)?;
        if !seen_activity_keys.insert(activity.key.clone()) {
            return Err(format!("{path}.key 重复: {}", activity.key));
        }
        validate_activity_executor(&activity.executor, &format!("{path}.executor"))?;
        validate_activity_ports(activity, &path)?;
        validate_activity_policies(activity, &path)?;
    }

    if !seen_activity_keys.contains(entry_activity_key) {
        return Err(format!(
            "lifecycle.entry_activity_key `{entry_activity_key}` 未出现在 lifecycle.activities 中"
        ));
    }
    if activities.len() >= 2 && transitions.is_empty() {
        return Err(
            "lifecycle.transitions 不能为空：多 activity lifecycle 必须显式声明 transition"
                .to_string(),
        );
    }

    for (index, transition) in transitions.iter().enumerate() {
        validate_activity_transition(
            transition,
            index,
            activities,
            &seen_activity_keys,
            entry_activity_key,
        )?;
    }

    Ok(())
}

fn validate_activity_executor(
    executor: &ActivityExecutorSpec,
    field_path: &str,
) -> Result<(), String> {
    match executor {
        ActivityExecutorSpec::Agent(spec) => {
            validate_identity(&format!("{field_path}.procedure_key"), &spec.procedure_key)?;
        }
        ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::ApiRequest(spec)) => {
            validate_non_empty(&format!("{field_path}.method"), &spec.method)?;
            validate_non_empty(&format!("{field_path}.url_template"), &spec.url_template)?;
        }
        ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(spec)) => {
            validate_non_empty(&format!("{field_path}.command"), &spec.command)?;
        }
        ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::OperationScript(spec)) => {
            if spec.language != "rhai_v1" {
                return Err(format!("{field_path}.language 当前只支持 `rhai_v1`"));
            }
            if spec.host_api_version != 1 {
                return Err(format!("{field_path}.host_api_version 当前只支持 V1"));
            }
            validate_non_empty(&format!("{field_path}.source"), &spec.source)?;
            if u64::try_from(spec.source.len()).unwrap_or(u64::MAX) > spec.limits.max_source_bytes {
                return Err(format!("{field_path}.source 超过 max_source_bytes"));
            }
            let mut operation_refs = std::collections::BTreeSet::new();
            for operation_ref in &spec.requested_operations {
                operation_ref
                    .validate()
                    .map_err(|error| format!("{field_path}.requested_operations: {error}"))?;
                let key = (
                    operation_ref.provider.namespace.as_str(),
                    operation_ref.provider.provider_key.as_str(),
                    operation_ref.operation_key.as_str(),
                    operation_ref.contract_version,
                );
                if !operation_refs.insert(key) {
                    return Err(format!(
                        "{field_path}.requested_operations 存在重复 exact OperationRef"
                    ));
                }
            }
            validate_operation_script_limits(&spec.limits, field_path)?;
        }
        ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(spec)) => {
            validate_identity(
                &format!("{field_path}.form_schema_key"),
                &spec.form_schema_key,
            )?;
        }
    }
    Ok(())
}

fn validate_operation_script_limits(
    limits: &OperationScriptExecutorLimits,
    field_path: &str,
) -> Result<(), String> {
    let valid = limits.timeout_ms > 0
        && limits.max_source_bytes > 0
        && limits.max_input_bytes > 0
        && limits.max_output_bytes > 0
        && limits.max_rhai_operations > 0
        && limits.max_call_levels > 0
        && limits.max_string_size > 0
        && limits.max_array_size > 0
        && limits.max_map_size > 0
        && limits.max_operation_calls > 0
        && limits.max_parallel_operations > 0
        && limits.max_parallel_operations <= limits.max_operation_calls;
    if !valid {
        return Err(format!(
            "{field_path}.limits 必须为有界正值且并行上限不能超过调用上限"
        ));
    }
    Ok(())
}

fn validate_activity_ports(activity: &ActivityDefinition, field_path: &str) -> Result<(), String> {
    let mut seen_output_port_keys = std::collections::BTreeSet::new();
    for (index, port) in activity.output_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.output_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.output_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_output_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.output_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    let mut seen_input_port_keys = std::collections::BTreeSet::new();
    for (index, port) in activity.input_ports.iter().enumerate() {
        validate_identity(&format!("{field_path}.input_ports[{index}].key"), &port.key)?;
        validate_non_empty(
            &format!("{field_path}.input_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_input_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.input_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }
    Ok(())
}

fn validate_activity_policies(
    activity: &ActivityDefinition,
    field_path: &str,
) -> Result<(), String> {
    if matches!(
        activity.iteration_policy.max_attempts,
        Some(max_attempts) if max_attempts == 0
    ) {
        return Err(format!(
            "{field_path}.iteration_policy.max_attempts 必须大于 0"
        ));
    }

    if let ActivityJoinPolicy::NOfM { n } = activity.join_policy
        && n == 0
    {
        return Err(format!("{field_path}.join_policy.n 必须大于 0"));
    }

    match &activity.completion_policy {
        ActivityCompletionPolicy::OutputPorts { required_ports } => {
            if required_ports.is_empty() {
                return Err(format!(
                    "{field_path}.completion_policy.required_ports 不能为空"
                ));
            }
            for (index, port_key) in required_ports.iter().enumerate() {
                validate_identity(
                    &format!("{field_path}.completion_policy.required_ports[{index}]"),
                    port_key,
                )?;
                if !activity
                    .output_ports
                    .iter()
                    .any(|port| port.key == *port_key)
                {
                    return Err(format!(
                        "{field_path}.completion_policy.required_ports[{index}] 引用了不存在的 output port: {port_key}"
                    ));
                }
            }
        }
        ActivityCompletionPolicy::HumanDecision { decision_port } => {
            validate_identity(
                &format!("{field_path}.completion_policy.decision_port"),
                decision_port,
            )?;
            if !activity
                .output_ports
                .iter()
                .any(|port| port.key == *decision_port)
            {
                return Err(format!(
                    "{field_path}.completion_policy.decision_port 引用了不存在的 output port: {decision_port}"
                ));
            }
        }
        ActivityCompletionPolicy::HookGate { hook_key } => {
            validate_identity(
                &format!("{field_path}.completion_policy.hook_key"),
                hook_key,
            )?;
        }
        ActivityCompletionPolicy::ExecutorTerminal | ActivityCompletionPolicy::OpenEnded => {}
    }
    Ok(())
}

fn validate_activity_transition(
    transition: &ActivityTransition,
    index: usize,
    activities: &[ActivityDefinition],
    activity_keys: &std::collections::BTreeSet<String>,
    entry_activity_key: &str,
) -> Result<(), String> {
    let field_path = format!("lifecycle.transitions[{index}]");
    validate_identity(&format!("{field_path}.from"), &transition.from)?;
    validate_identity(&format!("{field_path}.to"), &transition.to)?;
    if !activity_keys.contains(&transition.from) {
        return Err(format!(
            "{field_path}.from 引用了不存在的 activity: {}",
            transition.from
        ));
    }
    if !activity_keys.contains(&transition.to) {
        return Err(format!(
            "{field_path}.to 引用了不存在的 activity: {}",
            transition.to
        ));
    }
    if transition.from == transition.to
        && matches!(transition.condition, TransitionCondition::Always)
    {
        return Err(format!("{field_path} 不允许无条件自环"));
    }
    if transition.to == entry_activity_key && transition.from != entry_activity_key {
        validate_bounded_loop(transition, activities, &field_path)?;
    }
    validate_transition_condition(&transition.condition, activities, &field_path)?;

    for (binding_index, binding) in transition.artifact_bindings.iter().enumerate() {
        let binding_path = format!("{field_path}.artifact_bindings[{binding_index}]");
        let from_activity = binding
            .from_activity
            .as_deref()
            .unwrap_or(transition.from.as_str());
        validate_identity(&format!("{binding_path}.from_activity"), from_activity)?;
        validate_identity(&format!("{binding_path}.from_port"), &binding.from_port)?;
        validate_identity(&format!("{binding_path}.to_port"), &binding.to_port)?;
        let Some(source) = find_activity(activities, from_activity) else {
            return Err(format!(
                "{binding_path}.from_activity 引用了不存在的 activity: {from_activity}"
            ));
        };
        let Some(target) = find_activity(activities, &transition.to) else {
            return Err(format!(
                "{field_path}.to 引用了不存在的 activity: {}",
                transition.to
            ));
        };
        if !source
            .output_ports
            .iter()
            .any(|port| port.key == binding.from_port)
        {
            return Err(format!(
                "{binding_path}.from_port 引用了不存在的 output port: {}.{}",
                from_activity, binding.from_port
            ));
        }
        if !target
            .input_ports
            .iter()
            .any(|port| port.key == binding.to_port)
        {
            return Err(format!(
                "{binding_path}.to_port 引用了不存在的 input port: {}.{}",
                transition.to, binding.to_port
            ));
        }
    }

    Ok(())
}

fn validate_bounded_loop(
    transition: &ActivityTransition,
    activities: &[ActivityDefinition],
    field_path: &str,
) -> Result<(), String> {
    let target = find_activity(activities, &transition.to)
        .ok_or_else(|| format!("{field_path}.to 引用了不存在的 activity: {}", transition.to))?;
    let has_target_attempt_limit = target.iteration_policy.max_attempts.is_some();
    let has_transition_limit = transition.max_traversals.is_some();
    let has_structured_condition = !matches!(transition.condition, TransitionCondition::Always);
    if has_target_attempt_limit || has_transition_limit || has_structured_condition {
        Ok(())
    } else {
        Err(format!(
            "{field_path} 指向入口 activity 的循环 transition 必须由 max_attempts、max_traversals 或结构化条件约束"
        ))
    }
}

fn validate_transition_condition(
    condition: &TransitionCondition,
    activities: &[ActivityDefinition],
    field_path: &str,
) -> Result<(), String> {
    match condition {
        TransitionCondition::Always => {}
        TransitionCondition::ArtifactFieldEquals {
            activity,
            port,
            path,
            value: _,
        } => {
            validate_activity_output_port_ref(
                activities,
                activity,
                port,
                &format!("{field_path}.condition"),
            )?;
            validate_non_empty(&format!("{field_path}.condition.path"), path)?;
        }
        TransitionCondition::HumanDecisionEquals {
            activity,
            decision_port,
            value,
        } => {
            validate_activity_output_port_ref(
                activities,
                activity,
                decision_port,
                &format!("{field_path}.condition"),
            )?;
            validate_non_empty(&format!("{field_path}.condition.value"), value)?;
        }
        TransitionCondition::AgentSignalEquals {
            activity,
            signal_key,
            value: _,
        } => {
            validate_identity(&format!("{field_path}.condition.activity"), activity)?;
            if find_activity(activities, activity).is_none() {
                return Err(format!(
                    "{field_path}.condition.activity 引用了不存在的 activity: {activity}"
                ));
            }
            validate_identity(&format!("{field_path}.condition.signal_key"), signal_key)?;
        }
    }
    Ok(())
}

fn validate_activity_output_port_ref(
    activities: &[ActivityDefinition],
    activity_key: &str,
    port_key: &str,
    field_path: &str,
) -> Result<(), String> {
    validate_identity(&format!("{field_path}.activity"), activity_key)?;
    validate_identity(&format!("{field_path}.port"), port_key)?;
    let Some(activity) = find_activity(activities, activity_key) else {
        return Err(format!(
            "{field_path}.activity 引用了不存在的 activity: {activity_key}"
        ));
    };
    if !activity
        .output_ports
        .iter()
        .any(|port| port.key == port_key)
    {
        return Err(format!(
            "{field_path}.port 引用了不存在的 output port: {activity_key}.{port_key}"
        ));
    }
    Ok(())
}

fn find_activity<'a>(
    activities: &'a [ActivityDefinition],
    activity_key: &str,
) -> Option<&'a ActivityDefinition> {
    activities
        .iter()
        .find(|activity| activity.key == activity_key)
}

fn validate_contract(contract: &AgentProcedureContract, field_path: &str) -> Result<(), String> {
    validate_capability_config(
        &contract.capability_config,
        &format!("{field_path}.capability_config"),
    )?;

    for (index, binding) in contract.injection.context_bindings.iter().enumerate() {
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].locator"),
            &binding.locator,
        )?;
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].reason"),
            &binding.reason,
        )?;
    }

    let mut seen_rule_keys = std::collections::BTreeSet::new();
    for (index, rule) in contract.hook_rules.iter().enumerate() {
        validate_identity(&format!("{field_path}.hook_rules[{index}].key"), &rule.key)?;
        if rule.preset.is_none() && rule.script.is_none() {
            return Err(format!(
                "{field_path}.hook_rules[{index}] 必须指定 preset 或 script"
            ));
        }
        if !seen_rule_keys.insert(rule.key.clone()) {
            return Err(format!(
                "{field_path}.hook_rules[{index}].key 重复: {}",
                rule.key
            ));
        }
    }

    let mut seen_output_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.output_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.output_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.output_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_output_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.output_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    let mut seen_input_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.input_ports.iter().enumerate() {
        validate_identity(&format!("{field_path}.input_ports[{index}].key"), &port.key)?;
        validate_non_empty(
            &format!("{field_path}.input_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_input_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.input_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    Ok(())
}

fn validate_capability_config(config: &CapabilityConfig, field_path: &str) -> Result<(), String> {
    for (index, directive) in config.tool_directives.iter().enumerate() {
        let path = directive.path();
        let item_path = format!("{field_path}.tool_directives[{index}]");
        validate_identity(&format!("{item_path}.capability"), &path.capability)?;
        if let Some(tool) = &path.tool {
            validate_identity(&format!("{item_path}.tool"), tool)?;
        }
    }

    for (index, directive) in config.mount_directives.iter().enumerate() {
        let item_path = format!("{field_path}.mount_directives[{index}]");
        match directive {
            MountDirective::AddMount { mount } | MountDirective::ReplaceMount { mount } => {
                validate_identity(&format!("{item_path}.mount.id"), &mount.id)?;
                validate_non_empty(&format!("{item_path}.mount.provider"), &mount.provider)?;
                validate_non_empty(
                    &format!("{item_path}.mount.display_name"),
                    &mount.display_name,
                )?;
            }
            MountDirective::RemoveMount { mount_id } => {
                validate_identity(&format!("{item_path}.mount_id"), mount_id)?;
            }
            MountDirective::AddLink { link } => {
                validate_identity(
                    &format!("{item_path}.link.from_mount_id"),
                    &link.from_mount_id,
                )?;
                validate_identity(&format!("{item_path}.link.to_mount_id"), &link.to_mount_id)?;
            }
            MountDirective::RemoveLink {
                from_mount_id,
                from_path: _,
            } => {
                validate_identity(&format!("{item_path}.from_mount_id"), from_mount_id)?;
            }
            MountDirective::SetDefaultMount { mount_id } => {
                if let Some(mount_id) = mount_id {
                    validate_identity(&format!("{item_path}.mount_id"), mount_id)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_identity(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    if value.chars().any(char::is_whitespace) {
        return Err(format!("{field} 不能包含空白字符"));
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} 不能为空"));
    }
    Ok(())
}

#[cfg(test)]
mod operation_script_validation_tests {
    use crate::operation::OperationRef;

    use super::*;

    fn spec() -> OperationScriptExecutorSpec {
        OperationScriptExecutorSpec {
            language: "rhai_v1".to_string(),
            host_api_version: 1,
            source: "input".to_string(),
            input_binding: OperationScriptInputBinding::NodeInput,
            requested_operations: vec![
                OperationRef::new("workflow", "fixture", "lookup", 1).expect("operation ref"),
            ],
            limits: OperationScriptExecutorLimits::default(),
        }
    }

    #[test]
    fn operation_script_executor_accepts_v1_whole_node_input_contract() {
        validate_activity_executor(
            &ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::OperationScript(spec())),
            "activity.executor",
        )
        .expect("valid OperationScript executor");
    }

    #[test]
    fn operation_script_executor_rejects_duplicate_exact_refs() {
        let mut spec = spec();
        spec.requested_operations
            .push(spec.requested_operations[0].clone());
        let error = validate_activity_executor(
            &ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::OperationScript(spec)),
            "activity.executor",
        )
        .expect_err("duplicate refs");
        assert!(error.contains("重复 exact OperationRef"));
    }

    #[test]
    fn operation_script_executor_rejects_unbounded_limits() {
        let mut spec = spec();
        spec.limits.timeout_ms = 0;
        let error = validate_activity_executor(
            &ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::OperationScript(spec)),
            "activity.executor",
        )
        .expect_err("zero timeout");
        assert!(error.contains("有界正值"));
    }
}
