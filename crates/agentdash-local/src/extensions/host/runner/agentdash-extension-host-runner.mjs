import fs from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import vm from "node:vm";
import { pathToFileURL } from "node:url";

const extensions = new Map();
let defaultExtensionKey = null;
let currentInvocation = null;
let invocationDepth = 0;
let nextHostApiId = 1;
const pendingHostApi = new Map();
const MAX_INVOCATION_DEPTH = 32;

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function log(level, message) {
  send({ kind: "log", level, message: String(message) });
}

function toJsonValue(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (typeof value === "object") {
    const result = {};
    for (const [key, item] of Object.entries(value)) {
      if (typeof item !== "function" && typeof item !== "symbol" && typeof item !== "undefined") {
        result[key] = toJsonValue(item);
      }
    }
    return result;
  }
  return null;
}

function canonicalChannelKey(extensionKey, channelKey) {
  if (typeof channelKey !== "string" || channelKey.trim() === "") {
    throw new Error("channel_key is required");
  }
  return channelKey.includes(".") ? channelKey : `${extensionKey}.${channelKey}`;
}

function channelHandlerKey(channelKey, method) {
  return `${channelKey}#${method}`;
}

function normalizeChannelMethods(methods) {
  if (Array.isArray(methods)) {
    return methods;
  }
  if (methods && typeof methods === "object") {
    return Object.entries(methods).map(([name, method]) => ({ ...method, name: method.name ?? name }));
  }
  throw new Error("protocol channel methods must be an array or object");
}

function createExtensionContext(extensionKey) {
  const actions = new Map();
  const channels = new Map();
  const contributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    protocol_channels: [],
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
          return await requestHostApi("runtime.invoke", { target_action_key: actionKey, input: toJsonValue(input) }, extensionKey);
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
          return await requestHostApi("process.exec", { command, args: toJsonValue(args), options: toJsonValue(options) }, extensionKey);
        },
        async shell(command, options = {}) {
          return await requestHostApi("process.shell", { command, options: toJsonValue(options) }, extensionKey);
        },
      },
      channels: {
        async invoke(channelKey, method, input) {
          const canonical = canonicalChannelKey(extensionKey, channelKey);
          const local = findChannel(canonical, method);
          if (local) {
            return await invokeRegisteredChannel(local.extensionKey, canonical, method, input);
          }
          return await requestHostApi("extension.channel_invoke", {
            channel_key: channelKey,
            method,
            input: toJsonValue(input),
          }, extensionKey);
        },
        self(channelKey = "api") {
          const canonical = canonicalChannelKey(extensionKey, channelKey);
          return {
            async invoke(method, input) {
              return await invokeRegisteredChannel(extensionKey, canonical, method, input);
            },
          };
        },
        from(alias, channelKey = null) {
          return {
            async invoke(method, input) {
              const resolved = resolveDependencyChannel(extensionKey, alias, channelKey);
              const local = findChannel(resolved.channelKey, method);
              if (local) {
                return await invokeRegisteredChannel(local.extensionKey, resolved.channelKey, method, input);
              }
              return await requestHostApi("extension.channel_invoke", {
                dependency_alias: alias,
                channel_key: channelKey,
                method,
                input: toJsonValue(input),
              }, extensionKey);
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
    channels: {
      register(definition) {
        if (!definition || typeof definition.channel_key !== "string") {
          throw new Error("protocol channel must include channel_key");
        }
        const canonical = canonicalChannelKey(extensionKey, definition.channel_key);
        const methods = normalizeChannelMethods(definition.methods);
        if (methods.length === 0) {
          throw new Error("protocol channel must include at least one method");
        }
        const serializableMethods = [];
        for (const method of methods) {
          if (!method || typeof method.name !== "string" || typeof method.invoke !== "function") {
            throw new Error("protocol channel method must include name and invoke");
          }
          channels.set(channelHandlerKey(canonical, method.name), method);
          const { invoke, ...serializable } = method;
          serializableMethods.push(toJsonValue(serializable));
        }
        contributions.protocol_channels.push(toJsonValue({
          ...definition,
          channel_key: canonical,
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
  return { ctx, actions, channels, contributions };
}

function findAction(actionKey) {
  for (const [extensionKey, record] of extensions) {
    const action = record.actions.get(actionKey);
    if (action) return { extensionKey, action };
  }
  return null;
}

function findChannel(channelKey, method) {
  const key = channelHandlerKey(channelKey, method);
  for (const [extensionKey, record] of extensions) {
    if (record.channels.has(key)) return { extensionKey, handler: record.channels.get(key) };
  }
  return null;
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
    return stringArray(manifestAction?.permissions ?? record.actions.get(currentInvocation.actionKey)?.permissions);
  }
  if (currentInvocation.channelKey && currentInvocation.channelMethod) {
    const manifestChannel = (record.manifest?.protocol_channels ?? [])
      .find((channel) => channel?.channel_key === currentInvocation.channelKey);
    const manifestMethod = (manifestChannel?.methods ?? [])
      .find((method) => method?.name === currentInvocation.channelMethod);
    const handler = record.channels.get(channelHandlerKey(currentInvocation.channelKey, currentInvocation.channelMethod));
    return stringArray(manifestMethod?.permissions ?? handler?.permissions);
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
    channelKey: null,
    channelMethod: null,
  }, async () => {
    return toJsonValue(await action.invoke(toJsonValue(input)));
  });
}

async function invokeRegisteredChannel(extensionKey, channelKey, method, input) {
  const record = extensions.get(extensionKey);
  const handler = record?.channels.get(channelHandlerKey(channelKey, method));
  if (!handler) {
    throw new Error(`extension channel method is not registered: ${channelKey}.${method}`);
  }
  return await withInvocationContext({
    extensionKey,
    actionKey: null,
    channelKey,
    channelMethod: method,
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

function resolveDependencyChannel(extensionKey, alias, channelKey) {
  const record = extensions.get(extensionKey);
  if (!record) throw new Error(`extension is not active: ${extensionKey}`);
  const dependency = (record.manifest?.extension_dependencies ?? []).find((item) => item.alias === alias);
  if (!dependency) throw new Error(`extension dependency alias is not declared: ${alias}`);
  const requested = typeof channelKey === "string" ? channelKey.trim() : "";
  if (requested === "") {
    const first = dependency.channels?.[0];
    if (!first) throw new Error(`extension dependency has no channels: ${alias}`);
    return { channelKey: first, alias };
  }
  const matched = requested.includes(".")
    ? dependency.channels.find((item) => item === requested)
    : dependency.channels.find((item) => item.split(".").at(-1) === requested);
  if (!matched) throw new Error(`extension dependency channel is not declared: ${alias}.${requested}`);
  return { channelKey: matched, alias };
}

function invocationContextParams(extensionKey) {
  return {
    extension_key: extensionKey,
    action_key: currentInvocation?.extensionKey === extensionKey ? currentInvocation.actionKey : null,
    channel_key: currentInvocation?.extensionKey === extensionKey ? currentInvocation.channelKey : null,
    channel_method: currentInvocation?.extensionKey === extensionKey ? currentInvocation.channelMethod : null,
  };
}

async function requestHostApi(method, params, extensionKey) {
  const id = `host-api-${nextHostApiId++}`;
  send({
    kind: "host_api_request",
    id,
    method,
    params: toJsonValue({ ...invocationContextParams(extensionKey), ...params }),
  });
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingHostApi.delete(id);
      reject(new Error(`host api timeout: ${method}`));
    }, 30000);
    pendingHostApi.set(id, {
      resolve(value) {
        clearTimeout(timeout);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timeout);
        reject(error);
      },
    });
  });
}

async function loadExtension(bundlePath) {
  const source = await fs.readFile(bundlePath, "utf8");
  const moduleUrl = pathToFileURL(path.resolve(bundlePath)).href;
  const context = vm.createContext({
    console: {
      log: (...args) => log("info", args.join(" ")),
      warn: (...args) => log("warn", args.join(" ")),
      error: (...args) => log("error", args.join(" ")),
    },
    setTimeout,
    clearTimeout,
    structuredClone,
    TextDecoder,
    TextEncoder,
  });
  const module = new vm.SourceTextModule(source, {
    context,
    identifier: `${moduleUrl}?t=${Date.now()}`,
    initializeImportMeta(meta) {
      meta.url = moduleUrl;
    },
    importModuleDynamically(specifier) {
      throw new Error(`extension bundle must be self-contained; dynamic import blocked: ${specifier}`);
    },
  });
  await module.link((specifier) => {
    throw new Error(`extension bundle must be self-contained; import blocked: ${specifier}`);
  });
  await module.evaluate();
  const exported = module.namespace.default ?? module.namespace.extension;
  if (!exported || typeof exported !== "object") {
    throw new Error("extension bundle must export a default extension object");
  }
  return exported;
}

async function activate(params) {
  const extensionKey = params.extension_key;
  if (typeof extensionKey !== "string" || extensionKey.trim() === "") {
    throw new Error("extension_key is required");
  }
  await deactivate({ extension_key: extensionKey });
  const extension = await loadExtension(params.bundle_path);
  const { ctx, actions, channels, contributions } = createExtensionContext(extensionKey);
  const record = {
    extension,
    manifest: params.manifest,
    extensionKey,
    actions,
    channels,
    contributions,
  };
  extensions.set(extensionKey, record);
  defaultExtensionKey = extensionKey;
  try {
    if (typeof extension.activate === "function") {
      await extension.activate(ctx);
    }
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

async function invokeChannel(params) {
  const method = params.method;
  if (typeof method !== "string" || method.trim() === "") {
    throw new Error("extension channel method is required");
  }
  const scope = typeof params.extension_key === "string" ? params.extension_key : defaultExtensionKey;
  const channelKey = params.channel_key?.includes(".")
    ? params.channel_key
    : canonicalChannelKey(scope, params.channel_key);
  const found = findChannel(channelKey, method);
  if (!found) throw new Error(`extension channel method is not registered: ${channelKey}.${method}`);
  return await invokeRegisteredChannel(found.extensionKey, channelKey, method, params.input);
}

function healthPayload() {
  const defaultRecord = defaultExtensionKey ? extensions.get(defaultExtensionKey) : null;
  const actionKeys = [];
  const channelKeys = new Set();
  for (const record of extensions.values()) {
    actionKeys.push(...record.actions.keys());
    for (const key of record.channels.keys()) {
      channelKeys.add(key.split('#')[0]);
    }
  }
  return {
    active: extensions.size > 0,
    extension_id: defaultRecord?.manifest?.extension_id ?? null,
    action_keys: actionKeys.sort(),
    channel_keys: [...channelKeys].sort(),
    pid: process.pid,
  };
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
    case "invoke_channel":
      return await invokeChannel(message.params ?? {});
    case "health":
      return healthPayload();
    default:
      throw new Error(`unknown extension host method: ${message.method}`);
  }
}

rl.on("line", (line) => {
  void (async () => {
    const message = JSON.parse(line);
    if (message.kind === "host_api_response") {
      const pending = pendingHostApi.get(message.id);
      if (!pending) return;
      pendingHostApi.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.result ?? null);
      return;
    }
    if (message.kind !== "request") return;
    try {
      const result = await handleRequest(message);
      send({ kind: "response", id: message.id, result: toJsonValue(result) });
    } catch (error) {
      send({ kind: "response", id: message.id, error: error instanceof Error ? error.message : String(error) });
    }
  })().catch((error) => {
    send({ kind: "log", level: "error", message: error instanceof Error ? error.message : String(error) });
  });
});
