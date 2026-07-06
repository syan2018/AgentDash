import path from 'node:path';
import fs from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';

const VALID_API_MODES = new Set(['external', 'sidecar']);
const DEFAULT_DESKTOP_API_ORIGIN = 'http://127.0.0.1:17301';

export function runDesktopBuild(options) {
  const root = path.resolve(options.root);
  loadEnvFile(root);

  let config;
  try {
    config = parseDesktopBuildArgs(process.argv.slice(2), {
      ...options,
      root,
    });
  } catch (error) {
    console.error(`[desktop-build] ${error.message}`);
    process.exit(1);
  }
  if (config.help) {
    printHelp(options);
    return;
  }

  let rustBuild;
  try {
    rustBuild = configureRustBuild({
      root: config.root,
      sccacheMode: config.sccacheMode,
      sccacheDir: config.sccacheDir,
    });
  } catch (error) {
    console.error(`[desktop-build] ${error.message}`);
    process.exit(1);
  }
  const env = {
    ...rustBuild.env,
    AGENTDASH_DESKTOP_DEFAULT_API_MODE: config.apiMode,
    AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN: config.apiOrigin,
    AGENTDASH_DESKTOP_DEFAULTS_JSON: JSON.stringify(config.desktopDefaults),
    VITE_API_ORIGIN: config.apiOrigin,
  };
  if (config.apiMode === 'sidecar') {
    env.AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR = config.apiSidecar;
  } else {
    delete env.AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR;
  }

  const tauriArgs = [
    'exec',
    'tauri',
    'build',
    '--config',
    config.tauriConfigPath,
    ...config.passthrough,
  ];

  console.log(`[desktop-build] API mode: ${config.apiMode}`);
  console.log(`[desktop-build] API origin: ${config.apiOrigin}`);
  if (config.apiMode === 'sidecar') {
    console.log(`[desktop-build] API sidecar: ${config.apiSidecar}`);
  }
  if (config.desktopDefaultsPath) {
    console.log(`[desktop-build] desktop defaults: ${config.desktopDefaultsPath}`);
  }
  if (config.desktopDefaults.default_cloud_origin) {
    console.log(`[desktop-build] default cloud origin: ${config.desktopDefaults.default_cloud_origin}`);
  } else {
    console.log('[desktop-build] default cloud origin: 未配置');
  }
  console.log(`[desktop-build] rust cache: ${rustBuild.description}`);

  const pnpmSpawn = resolvePnpmSpawn(tauriArgs);
  const child = spawn(pnpmSpawn.command, pnpmSpawn.args, {
    cwd: config.root,
    env,
    stdio: 'inherit',
    windowsHide: false,
  });

  child.on('error', (error) => {
    console.error(error);
    process.exit(1);
  });

  child.on('exit', (code, signal) => {
    if (signal) {
      console.error(`[desktop-build] tauri build 被信号中止: ${signal}`);
      process.exit(1);
    }
    if (code === 0) {
      printDesktopArtifactBoundary(config.root);
    }
    process.exit(code ?? 0);
  });
}

function parseDesktopBuildArgs(args, options) {
  const root = path.resolve(options.root);
  const env = process.env;
  let apiMode = normalizeApiMode(
    env.AGENTDASH_DESKTOP_DEFAULT_API_MODE
      || env.AGENTDASH_DESKTOP_API_MODE
      || options.defaultApiMode
      || 'external',
  );
  let apiOrigin = normalizeOptionalOrigin(
    env.AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN
      || env.AGENTDASH_DESKTOP_API_ORIGIN
      || options.defaultApiOrigin,
    '--api-origin',
  );
  let apiSidecar = normalizeOptionalValue(
    env.AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR
      || env.AGENTDASH_DESKTOP_API_SIDECAR
      || options.defaultApiSidecar,
  );
  let defaultCloudOrigin = normalizeOptionalOrigin(
    env.AGENTDASH_DEFAULT_CLOUD_ORIGIN
      || options.defaultCloudOrigin,
    '--default-cloud-origin',
  );
  let desktopDefaultsPath = null;
  let sccacheMode = 'auto';
  let sccacheDir = env.SCCACHE_DIR || null;
  let help = false;
  const passthrough = [];

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--help' || arg === '-h') {
      help = true;
      continue;
    }
    if (arg.startsWith('--api-mode=')) {
      apiMode = normalizeApiMode(arg.slice('--api-mode='.length));
      continue;
    }
    if (arg === '--api-mode') {
      apiMode = normalizeApiMode(readNextValue(args, ++index, arg));
      continue;
    }
    if (arg.startsWith('--api-origin=')) {
      apiOrigin = normalizeRequiredOrigin(arg.slice('--api-origin='.length), '--api-origin');
      continue;
    }
    if (arg === '--api-origin') {
      apiOrigin = normalizeRequiredOrigin(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--api-sidecar=')) {
      apiSidecar = normalizeRequiredValue(arg.slice('--api-sidecar='.length), arg);
      continue;
    }
    if (arg === '--api-sidecar') {
      apiSidecar = normalizeRequiredValue(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--default-cloud-origin=')) {
      defaultCloudOrigin = normalizeOptionalOrigin(
        arg.slice('--default-cloud-origin='.length),
        '--default-cloud-origin',
      );
      continue;
    }
    if (arg === '--default-cloud-origin') {
      defaultCloudOrigin = normalizeOptionalOrigin(
        readNextValue(args, ++index, arg),
        '--default-cloud-origin',
      );
      continue;
    }
    if (arg.startsWith('--desktop-defaults=')) {
      desktopDefaultsPath = normalizeRequiredValue(arg.slice('--desktop-defaults='.length), arg);
      continue;
    }
    if (arg === '--desktop-defaults') {
      desktopDefaultsPath = normalizeRequiredValue(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg === '--sccache') {
      sccacheMode = 'required';
      continue;
    }
    if (arg === '--no-sccache') {
      sccacheMode = 'disabled';
      continue;
    }
    if (arg.startsWith('--sccache-dir=')) {
      sccacheDir = normalizeRequiredValue(arg.slice('--sccache-dir='.length), arg);
      continue;
    }
    if (arg === '--sccache-dir') {
      sccacheDir = normalizeRequiredValue(readNextValue(args, ++index, arg), arg);
      continue;
    }
    passthrough.push(arg);
  }

  if (!help && apiMode === 'sidecar' && !apiSidecar) {
    throw new Error('--api-mode sidecar 需要同时提供 --api-sidecar');
  }

  let desktopDefaults = loadDesktopDefaults({
    defaultCloudOrigin,
    defaultsPath: desktopDefaultsPath,
    root,
  });
  defaultCloudOrigin = defaultCloudOrigin || desktopDefaults.default_cloud_origin || null;

  if (!apiOrigin && apiMode === 'external') {
    apiOrigin = defaultCloudOrigin;
  } else if (!apiOrigin) {
    apiOrigin = DEFAULT_DESKTOP_API_ORIGIN;
  }

  if (!help) {
    validateDesktopApiConfig({ apiMode, apiOrigin });
  }
  if (!desktopDefaults.default_cloud_origin && apiMode === 'external' && apiOrigin) {
    desktopDefaults = {
      ...desktopDefaults,
      default_cloud_origin: apiOrigin,
    };
  }

  return {
    apiMode,
    apiOrigin,
    apiSidecar,
    desktopDefaults,
    desktopDefaultsPath,
    help,
    passthrough,
    root,
    sccacheDir,
    sccacheMode,
    tauriConfigPath: options.tauriConfigPath,
  };
}

function printHelp(options) {
  const defaultMode = options.defaultApiMode || 'external';
  const defaultOrigin = options.defaultApiOrigin || 'AGENTDASH_DEFAULT_CLOUD_ORIGIN / --api-origin';
  console.log('用法: node ./scripts/desktop-build.js [build-options] [...tauri-build-options]');
  console.log('');
  console.log('AgentDash 桌面端构建入口。');
  console.log('');
  console.log('Build options:');
  console.log(`  --api-mode <external|sidecar>          桌面壳默认 API 模式，默认 ${defaultMode}`);
  console.log(`  --api-origin <url>                    API origin，默认 ${defaultOrigin}`);
  console.log('  --api-sidecar <command>               sidecar 模式下启动的 API 可执行文件');
  console.log('  --desktop-defaults <path>             携带进安装包的桌面默认配置 JSON');
  console.log('  --default-cloud-origin <url>          快速设置 desktop defaults 中的默认云端 server origin');
  console.log('  --sccache                             要求使用 sccache');
  console.log('  --no-sccache                          关闭 RUSTC_WRAPPER');
  console.log('  --sccache-dir <path>                  指定 SCCACHE_DIR');
  console.log('  -h, --help                            显示帮助');
  console.log('');
  console.log('默认读取仓库根目录 .env；shell 环境变量和 CLI 参数优先级更高。');
  console.log('其他参数会原样传给 pnpm exec tauri build。');
}

function loadEnvFile(root) {
  const envPath = path.join(root, '.env');
  if (!fs.existsSync(envPath)) {
    return;
  }

  const content = fs.readFileSync(envPath, 'utf8');
  for (const line of content.split(/\r?\n/)) {
    const entry = parseEnvLine(line);
    if (!entry) continue;
    const { key, value } = entry;
    if (process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

function parseEnvLine(line) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith('#')) {
    return null;
  }

  const normalized = trimmed.startsWith('export ') ? trimmed.slice('export '.length).trimStart() : trimmed;
  const separatorIndex = normalized.indexOf('=');
  if (separatorIndex <= 0) {
    return null;
  }

  const key = normalized.slice(0, separatorIndex).trim();
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
    return null;
  }

  const rawValue = normalized.slice(separatorIndex + 1).trim();
  return {
    key,
    value: parseEnvValue(rawValue),
  };
}

function parseEnvValue(value) {
  if (!value) {
    return '';
  }
  const quote = value[0];
  if ((quote === '"' || quote === "'") && value.endsWith(quote)) {
    const inner = value.slice(1, -1);
    if (quote === '"') {
      return inner
        .replace(/\\n/g, '\n')
        .replace(/\\r/g, '\r')
        .replace(/\\t/g, '\t')
        .replace(/\\"/g, '"')
        .replace(/\\\\/g, '\\');
    }
    return inner;
  }
  return stripInlineEnvComment(value).trim();
}

function stripInlineEnvComment(value) {
  for (let index = 0; index < value.length; index += 1) {
    if (value[index] === '#' && (index === 0 || /\s/.test(value[index - 1]))) {
      return value.slice(0, index);
    }
  }
  return value;
}

function readNextValue(values, index, flagName) {
  const value = values[index];
  if (!value) {
    throw new Error(`${flagName} 缺少取值`);
  }
  return value;
}

function normalizeApiMode(value) {
  const normalized = String(value || '').trim().toLowerCase();
  if (!VALID_API_MODES.has(normalized)) {
    throw new Error(`未知桌面端 API mode: ${value}`);
  }
  return normalized;
}

function normalizeRequiredOrigin(value, flagName) {
  const origin = normalizeOptionalOrigin(value, flagName);
  if (!origin) {
    throw new Error(`${flagName} 不能为空`);
  }
  return origin;
}

function normalizeOptionalOrigin(value, flagName) {
  const normalized = normalizeOptionalValue(value);
  if (!normalized) {
    return null;
  }
  let parsed;
  try {
    parsed = new URL(normalized);
  } catch (error) {
    throw new Error(`${flagName} 无效: ${error.message}`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`${flagName} 只支持 http:// 或 https://`);
  }
  if (parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new Error(`${flagName} 必须是 origin，不应包含认证信息、query 或 hash`);
  }
  parsed.pathname = parsed.pathname.replace(/\/+$/, '') || '/';
  if (parsed.pathname !== '/') {
    throw new Error(`${flagName} 必须是 origin，不应包含 path`);
  }
  return parsed.origin;
}

function loadDesktopDefaults({ defaultCloudOrigin, defaultsPath, root }) {
  let defaults = {};
  if (defaultsPath) {
    const resolved = path.isAbsolute(defaultsPath) ? defaultsPath : path.resolve(root, defaultsPath);
    let parsed;
    try {
      parsed = JSON.parse(fs.readFileSync(resolved, 'utf8'));
    } catch (error) {
      throw new Error(`读取 --desktop-defaults 失败: ${error.message}`);
    }
    defaults = normalizeDesktopDefaults(parsed, `--desktop-defaults ${resolved}`);
  }
  if (defaultCloudOrigin) {
    defaults = {
      ...defaults,
      default_cloud_origin: defaultCloudOrigin,
    };
  }
  return normalizeDesktopDefaults(defaults, 'desktop defaults');
}

function normalizeDesktopDefaults(value, sourceLabel) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${sourceLabel} 必须是 JSON object`);
  }
  const result = {};
  if (Object.prototype.hasOwnProperty.call(value, 'default_cloud_origin')) {
    const origin = normalizeOptionalOrigin(value.default_cloud_origin, `${sourceLabel}.default_cloud_origin`);
    if (origin) {
      result.default_cloud_origin = origin;
    }
  }
  return result;
}

function validateDesktopApiConfig({ apiMode, apiOrigin }) {
  if (apiMode === 'external') {
    if (!apiOrigin) {
      throw new Error('桌面端默认使用 external API mode；请通过 AGENTDASH_DEFAULT_CLOUD_ORIGIN、--api-origin 或 --default-cloud-origin 配置远端 server origin');
    }
    return;
  }

  if (apiMode === 'sidecar' && !isDesktopLoopbackOrigin(apiOrigin)) {
    throw new Error(`sidecar Desktop API origin 必须是 ${DEFAULT_DESKTOP_API_ORIGIN}`);
  }
}

function isDesktopLoopbackOrigin(origin) {
  return origin === DEFAULT_DESKTOP_API_ORIGIN;
}

function normalizeRequiredValue(value, flagName) {
  const normalized = normalizeOptionalValue(value);
  if (!normalized) {
    throw new Error(`${flagName} 不能为空`);
  }
  return normalized;
}

function configureRustBuild(options) {
  const env = { ...process.env };
  const configuredSccacheDir = normalizeOptionalValue(options.sccacheDir);
  if (configuredSccacheDir) {
    env.SCCACHE_DIR = path.isAbsolute(configuredSccacheDir)
      ? configuredSccacheDir
      : path.resolve(options.root, configuredSccacheDir);
  }

  if (options.sccacheMode === 'disabled') {
    delete env.RUSTC_WRAPPER;
    return {
      description: '已关闭',
      env,
    };
  }

  const existingWrapper = normalizeOptionalValue(env.RUSTC_WRAPPER);
  if (existingWrapper && options.sccacheMode !== 'required') {
    return {
      description: `RUSTC_WRAPPER=${existingWrapper}${formatCacheDirSuffix(env.SCCACHE_DIR)}`,
      env,
    };
  }

  const sccachePath = resolveExecutable('sccache', options.root);
  if (sccachePath) {
    env.RUSTC_WRAPPER = sccachePath;
    return {
      description: formatSccacheDescription(sccachePath, env.SCCACHE_DIR),
      env,
    };
  }

  if (options.sccacheMode === 'required') {
    throw new Error('--sccache 需要先安装 sccache，并确保 sccache 在 PATH 中可执行');
  }

  return {
    description: '未检测到 sccache，已退化为普通 rustc',
    env,
  };
}

function normalizeOptionalValue(value) {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function resolveExecutable(name, root) {
  const command = process.platform === 'win32' ? 'where.exe' : 'sh';
  const args = process.platform === 'win32' ? [name] : ['-lc', `command -v ${name}`];
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    return null;
  }
  const firstLine = result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
  return firstLine || null;
}

function resolvePnpmSpawn(args) {
  if (process.platform === 'win32') {
    return {
      command: 'cmd.exe',
      args: ['/d', '/s', '/c', 'pnpm.cmd', ...args],
    };
  }
  return {
    command: 'pnpm',
    args,
  };
}

function formatSccacheDescription(sccachePath, cacheDir) {
  return `sccache (${sccachePath})${formatCacheDirSuffix(cacheDir)}`;
}

function formatCacheDirSuffix(cacheDir) {
  const normalized = normalizeOptionalValue(cacheDir);
  return normalized ? `，SCCACHE_DIR=${normalized}` : '';
}

function printDesktopArtifactBoundary(root) {
  const releaseDir = path.join(root, 'target', 'release');
  const nsisDir = path.join(releaseDir, 'bundle', 'nsis');
  const setupExeFiles = listFiles(nsisDir, (file) => file.toLowerCase().endsWith('.exe'));
  const updaterFiles = listFiles(nsisDir, (file) => {
    const lower = file.toLowerCase();
    return lower.endsWith('.nsis.zip') || lower.endsWith('.msi.zip');
  });
  const appExeCandidates = [
    path.join(releaseDir, 'AgentDash.exe'),
    path.join(releaseDir, 'agentdash-local-tauri.exe'),
  ].filter((file) => fs.existsSync(file));

  console.log('[desktop-build] 产物边界:');
  if (setupExeFiles.length > 0) {
    for (const file of setupExeFiles) {
      console.log(`[desktop-build]   setup exe: ${file}`);
    }
  } else {
    console.log(`[desktop-build]   setup exe: 未在 ${nsisDir} 发现 NSIS exe`);
  }

  if (updaterFiles.length > 0) {
    for (const file of updaterFiles) {
      const signature = `${file}.sig`;
      console.log(`[desktop-build]   updater artifact: ${file}`);
      if (fs.existsSync(signature)) {
        console.log(`[desktop-build]   updater signature: ${signature}`);
      } else {
        console.log(`[desktop-build]   updater signature: 未发现 ${signature}`);
      }
    }
  } else {
    console.log(`[desktop-build]   updater artifact: 未在 ${nsisDir} 发现 *.nsis.zip 或 *.msi.zip`);
  }

  if (appExeCandidates.length > 0) {
    for (const file of appExeCandidates) {
      console.log(`[desktop-build]   app exe: ${file}`);
    }
  } else {
    console.log(`[desktop-build]   app exe: 未在 ${releaseDir} 发现 AgentDash.exe 或 agentdash-local-tauri.exe`);
  }
}

function listFiles(dir, predicate) {
  if (!fs.existsSync(dir)) {
    return [];
  }
  return fs.readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.join(dir, entry.name))
    .filter(predicate)
    .sort();
}
