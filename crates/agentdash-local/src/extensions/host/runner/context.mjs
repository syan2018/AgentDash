const MAX_INVOCATION_DEPTH = 32;

export function createExtensionRuntime({
  loadExtension,
  requestHostApi,
  toJsonValue,
}) {
  const extensions = new Map();
  let defaultExtensionKey = null;
  let currentInvocation = null;
  let invocationDepth = 0;

  function canonicalProtocolKey(extensionKey, protocolKey) {
    if (typeof protocolKey !== "string" || protocolKey.trim() === "") {
      throw new Error("protocol_key is required");
    }
    return protocolKey.includes(".") ? protocolKey : `${extensionKey}.${protocolKey}`;
  }

  function protocolHandlerKey(protocolKey, method) {
    return `${protocolKey}#${method}`;
  }

  function normalizeChannelMethods(methods) {
    if (Array.isArray(methods)) {
      return methods;
    }
    if (methods && typeof methods === "object") {
      return Object.entries(methods).map(([name, method]) => ({ ...method, name: method.name ?? name }));
    }
    throw new Error("protocol methods must be an array or object");
  }

  function createExtensionContext(extensionKey) {
    const actions = new Map();
    const protocols = new Map();
    const contributions = {
      commands: [],
      flags: [],
      runtime_actions: [],
      protocols: [],
      workspace_panels: [],
      permissions: [],
    };
    const ctx = {
      api: {
        runtime: {
          async invoke(actionKey, input) {
            const local = findAction(actionKey);
            if (local) {
              ensureRuntimeInvokeAllowed(extensionKey, local.extensionKey, actionKey);
              return await invokeRegisteredAction(local.extensionKey, local.action, input);
            }
            ensureRuntimeInvokeAllowed(extensionKey, null, actionKey);
            throw new Error(`runtime action is not loaded in current extension host: ${actionKey}`);
          },
        },
        local: {
          async getProfile() {
            return await requestHostApi("local.get_profile", {}, extensionKey);
          },
        },
        http: {
          async fetch(url, options = {}) {
            return await requestHostApi("http.fetch", { url, options: toJsonValue(options) }, extensionKey);
          },
          async fetchJson(url, options = {}) {
            const response = await requestHostApi("http.fetch", { url, options: toJsonValue(options) }, extensionKey);
            if (response && typeof response === "object" && typeof response.body === "string") {
              return JSON.parse(response.body);
            }
            return response;
          },
        },
        workspace: {
          async readText(path) {
            return await requestHostApi("workspace.read_text", { path }, extensionKey);
          },
          async writeText(path, content) {
            return await requestHostApi("workspace.write_text", { path, content }, extensionKey);
          },
          async list(path) {
            return await requestHostApi("workspace.list", { path }, extensionKey);
          },
          async stat(path) {
            return await requestHostApi("workspace.stat", { path }, extensionKey);
          },
        },
        env: {
          async get(name) {
            return await requestHostApi("env.get", { name }, extensionKey);
          },
        },
        process: {
          async exec(command, args = [], options = {}) {
            return await requestHostApi(
              "process.exec",
              { command, args: toJsonValue(args), options: toJsonValue(options) },
              extensionKey,
            );
          },
          async shell(command, options = {}) {
            return await requestHostApi("process.shell", { command, options: toJsonValue(options) }, extensionKey);
          },
        },
        protocols: {
          async invoke(protocolKey, method, input) {
            const canonical = canonicalProtocolKey(extensionKey, protocolKey);
            const local = findChannel(canonical, method);
            if (local) {
              return await invokeRegisteredChannel(local.extensionKey, canonical, method, input);
            }
            throwChannelMethodNotLoaded(canonical, method);
          },
          self(protocolKey = "api") {
            const canonical = canonicalProtocolKey(extensionKey, protocolKey);
            return {
              async invoke(method, input) {
                return await invokeRegisteredChannel(extensionKey, canonical, method, input);
              },
            };
          },
          from(alias, protocolKey = null) {
            return {
              async invoke(method, input) {
                const resolved = resolveDependencyProtocol(extensionKey, alias, protocolKey);
                const local = findChannel(resolved.protocolKey, method);
                if (local) {
                  return await invokeRegisteredChannel(local.extensionKey, resolved.protocolKey, method, input);
                }
                throwChannelMethodNotLoaded(resolved.protocolKey, method);
              },
            };
          },
        },
      },
      commands: {
        registerCommand(definition) {
          contributions.commands.push(toJsonValue(definition));
        },
      },
      flags: {
        registerFlag(definition) {
          contributions.flags.push(toJsonValue(definition));
        },
      },
      runtime: {
        registerAction(definition) {
          if (!definition || typeof definition.action_key !== "string" || typeof definition.invoke !== "function") {
            throw new Error("runtime action must include action_key and invoke");
          }
          actions.set(definition.action_key, definition);
          const { invoke, ...serializable } = definition;
          contributions.runtime_actions.push(toJsonValue(serializable));
        },
      },
      protocols: {
        register(definition) {
          if (!definition || typeof definition.protocol_key !== "string") {
            throw new Error("protocol must include protocol_key");
          }
          const canonical = canonicalProtocolKey(extensionKey, definition.protocol_key);
          const methods = normalizeChannelMethods(definition.methods);
          if (methods.length === 0) {
            throw new Error("protocol must include at least one method");
          }
          const serializableMethods = [];
          for (const method of methods) {
            if (!method || typeof method.name !== "string" || typeof method.invoke !== "function") {
              throw new Error("protocol method must include name and invoke");
            }
            protocols.set(protocolHandlerKey(canonical, method.name), method);
            const { invoke, ...serializable } = method;
            serializableMethods.push(toJsonValue(serializable));
          }
          contributions.protocols.push(toJsonValue({
            ...definition,
            protocol_key: canonical,
            methods: serializableMethods,
          }));
        },
      },
      workspace: {
        registerPanel(definition) {
          contributions.workspace_panels.push(toJsonValue(definition));
        },
      },
      permissions: {
        require(permission) {
          contributions.permissions.push(toJsonValue(permission));
        },
      },
      contributions,
    };
    return { ctx, actions, protocols, contributions };
  }

  function findAction(actionKey) {
    for (const [extensionKey, record] of extensions) {
      const action = record.actions.get(actionKey);
      if (action) return { extensionKey, action };
    }
    return null;
  }

  function findChannel(protocolKey, method) {
    const key = protocolHandlerKey(protocolKey, method);
    for (const [extensionKey, record] of extensions) {
      if (record.protocols.has(key)) return { extensionKey, handler: record.protocols.get(key) };
    }
    return null;
  }

  function throwChannelMethodNotLoaded(protocolKey, method) {
    throw new Error(`extension protocol method is not loaded in current extension host: ${protocolKey}.${method}`);
  }

  function ensureRuntimeInvokeAllowed(consumerExtensionKey, targetExtensionKey, targetActionKey) {
    if (targetExtensionKey && consumerExtensionKey === targetExtensionKey) return;
    const permissions = currentInvocationPermissions(consumerExtensionKey);
    if (permissions.includes("runtime.invoke") || permissions.includes(`runtime.invoke:${targetActionKey}`)) {
      return;
    }
    throw new Error(`runtime.invoke:${targetActionKey} permission is not declared`);
  }

  function currentInvocationPermissions(extensionKey) {
    if (!currentInvocation || currentInvocation.extensionKey !== extensionKey) return [];
    const record = extensions.get(extensionKey);
    if (!record) return [];
    if (currentInvocation.actionKey) {
      const manifestAction = (record.manifest?.runtime_actions ?? [])
        .find((action) => action?.action_key === currentInvocation.actionKey);
      return stringArray(manifestAction?.permissions);
    }
    if (currentInvocation.protocolKey && currentInvocation.protocolMethod) {
      const manifestChannel = (record.manifest?.protocols ?? [])
        .find((channel) => channel?.protocol_key === currentInvocation.protocolKey);
      const manifestMethod = (manifestChannel?.methods ?? [])
        .find((method) => method?.name === currentInvocation.protocolMethod);
      return stringArray(manifestMethod?.permissions);
    }
    return [];
  }

  function stringArray(value) {
    return Array.isArray(value) ? value.filter((item) => typeof item === "string") : [];
  }

  async function invokeRegisteredAction(extensionKey, action, input) {
    return await withInvocationContext({
      extensionKey,
      actionKey: action.action_key,
      protocolKey: null,
      protocolMethod: null,
    }, async () => {
      return toJsonValue(await action.invoke(toJsonValue(input)));
    });
  }

  async function invokeRegisteredChannel(extensionKey, protocolKey, method, input) {
    const record = extensions.get(extensionKey);
    const handler = record?.protocols.get(protocolHandlerKey(protocolKey, method));
    if (!handler) {
      throwChannelMethodNotLoaded(protocolKey, method);
    }
    return await withInvocationContext({
      extensionKey,
      actionKey: null,
      protocolKey,
      protocolMethod: method,
    }, async () => {
      return toJsonValue(await handler.invoke(toJsonValue(input)));
    });
  }

  async function withInvocationContext(context, invoke) {
    if (invocationDepth >= MAX_INVOCATION_DEPTH) {
      throw new Error(`extension invocation depth exceeded: ${MAX_INVOCATION_DEPTH}`);
    }
    const previous = currentInvocation;
    currentInvocation = context;
    invocationDepth += 1;
    try {
      return await invoke();
    } finally {
      invocationDepth -= 1;
      currentInvocation = previous;
    }
  }

  function resolveDependencyProtocol(extensionKey, alias, protocolKey) {
    const record = extensions.get(extensionKey);
    if (!record) throw new Error(`extension is not active: ${extensionKey}`);
    const dependency = (record.manifest?.extension_dependencies ?? []).find((item) => item.alias === alias);
    if (!dependency) throw new Error(`extension dependency alias is not declared: ${alias}`);
    const requested = typeof protocolKey === "string" ? protocolKey.trim() : "";
    if (requested === "") {
      const first = dependency.protocols?.[0];
      if (!first) throw new Error(`extension dependency has no protocols: ${alias}`);
      return { protocolKey: first, alias };
    }
    const matched = requested.includes(".")
      ? dependency.protocols.find((item) => item === requested)
      : dependency.protocols.find((item) => item.split(".").at(-1) === requested);
    if (!matched) throw new Error(`extension dependency channel is not declared: ${alias}.${requested}`);
    return { protocolKey: matched, alias };
  }

  function invocationContextParams(extensionKey) {
    return {
      extension_key: extensionKey,
      action_key: currentInvocation?.extensionKey === extensionKey ? currentInvocation.actionKey : null,
      protocol_key: currentInvocation?.extensionKey === extensionKey ? currentInvocation.protocolKey : null,
      protocol_method: currentInvocation?.extensionKey === extensionKey ? currentInvocation.protocolMethod : null,
    };
  }

  async function activate(params) {
    const extensionKey = params.extension_key;
    if (typeof extensionKey !== "string" || extensionKey.trim() === "") {
      throw new Error("extension_key is required");
    }
    await deactivate({ extension_key: extensionKey });
    const extension = await loadExtension(params.bundle_path);
    const { ctx, actions, protocols, contributions } = createExtensionContext(extensionKey);
    const record = {
      extension,
      manifest: params.manifest,
      extensionKey,
      actions,
      protocols,
      contributions,
    };
    extensions.set(extensionKey, record);
    defaultExtensionKey = extensionKey;
    try {
      if (typeof extension.activate === "function") {
        await extension.activate(ctx);
      }
      enforceManifestSurface(record);
    } catch (error) {
      extensions.delete(extensionKey);
      if (defaultExtensionKey === extensionKey) defaultExtensionKey = extensions.keys().next().value ?? null;
      throw error;
    }
    return healthPayload();
  }

  async function deactivate(params = {}) {
    const extensionKey = typeof params.extension_key === "string" ? params.extension_key : null;
    if (extensionKey) {
      const record = extensions.get(extensionKey);
      if (record?.extension && typeof record.extension.deactivate === "function") {
        await record.extension.deactivate();
      }
      extensions.delete(extensionKey);
      if (defaultExtensionKey === extensionKey) defaultExtensionKey = extensions.keys().next().value ?? null;
      return healthPayload();
    }
    for (const record of extensions.values()) {
      if (record?.extension && typeof record.extension.deactivate === "function") {
        await record.extension.deactivate();
      }
    }
    extensions.clear();
    defaultExtensionKey = null;
    return healthPayload();
  }

  async function invokeAction(params) {
    const actionKey = params.action_key;
    const found = findAction(actionKey);
    if (!found) throw new Error(`extension action is not registered: ${actionKey}`);
    return await invokeRegisteredAction(found.extensionKey, found.action, params.input);
  }

  async function invokeProtocol(params) {
    const method = params.method;
    if (typeof method !== "string" || method.trim() === "") {
      throw new Error("extension protocol method is required");
    }
    const scope = typeof params.extension_key === "string" ? params.extension_key : defaultExtensionKey;
    const protocolKey = params.protocol_key?.includes(".")
      ? params.protocol_key
      : canonicalProtocolKey(scope, params.protocol_key);
    const found = findChannel(protocolKey, method);
    if (!found) throwChannelMethodNotLoaded(protocolKey, method);
    return await invokeRegisteredChannel(found.extensionKey, protocolKey, method, params.input);
  }

  function healthPayload() {
    const defaultRecord = defaultExtensionKey ? extensions.get(defaultExtensionKey) : null;
    const actionKeys = [];
    const protocolKeys = new Set();
    for (const record of extensions.values()) {
      actionKeys.push(...record.actions.keys());
      for (const key of record.protocols.keys()) {
        protocolKeys.add(key.split("#")[0]);
      }
    }
    return {
      active: extensions.size > 0,
      extension_id: defaultRecord?.manifest?.extension_id ?? null,
      action_keys: actionKeys.sort(),
      protocol_keys: [...protocolKeys].sort(),
      pid: process.pid,
    };
  }

  function enforceManifestSurface(record) {
    const manifestActions = new Set((record.manifest?.runtime_actions ?? [])
      .map((action) => typeof action?.action_key === "string" ? action.action_key : null)
      .filter(Boolean));
    for (const actionKey of record.actions.keys()) {
      if (!manifestActions.has(actionKey)) {
        throw new Error(`extension action is registered but not declared in manifest: ${actionKey}`);
      }
    }
    for (const actionKey of manifestActions) {
      if (!record.actions.has(actionKey)) {
        throw new Error(`extension action is declared in manifest but not registered: ${actionKey}`);
      }
    }

    const manifestMethods = new Set();
    for (const channel of record.manifest?.protocols ?? []) {
      if (typeof channel?.protocol_key !== "string") continue;
      const protocolKey = canonicalProtocolKey(record.extensionKey, channel.protocol_key);
      for (const method of channel.methods ?? []) {
        if (typeof method?.name === "string") {
          manifestMethods.add(protocolHandlerKey(protocolKey, method.name));
        }
      }
    }
    for (const handlerKey of record.protocols.keys()) {
      if (!manifestMethods.has(handlerKey)) {
        throw new Error(`extension protocol method is registered but not declared in manifest: ${handlerKey.replace("#", ".")}`);
      }
    }
    for (const handlerKey of manifestMethods) {
      if (!record.protocols.has(handlerKey)) {
        throw new Error(`extension protocol method is declared in manifest but not registered: ${handlerKey.replace("#", ".")}`);
      }
    }
  }

  async function handleRequest(message) {
    switch (message.method) {
      case "activate":
        return await activate(message.params ?? {});
      case "reload":
        await deactivate({ extension_key: message.params?.extension_key });
        return await activate(message.params ?? {});
      case "deactivate":
        return await deactivate(message.params ?? {});
      case "invoke_action":
        return await invokeAction(message.params ?? {});
      case "invoke_protocol":
        return await invokeProtocol(message.params ?? {});
      case "health":
        return healthPayload();
      default:
        throw new Error(`unknown extension host method: ${message.method}`);
    }
  }

  return {
    handleRequest,
    invocationContextParams,
  };
}
