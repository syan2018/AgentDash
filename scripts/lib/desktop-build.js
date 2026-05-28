import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';

const VALID_API_MODES = new Set(['builtin', 'external', 'sidecar']);

export function runDesktopBuild(options) {
  let config;
  try {
    config = parseDesktopBuildArgs(process.argv.slice(2), options);
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
  console.log(`[desktop-build] rust cache: ${rustBuild.description}`);

  const child = spawn(resolvePnpmCommand(), tauriArgs, {
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
      || 'builtin',
  );
  let apiOrigin = normalizeOrigin(
    env.AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN
      || env.AGENTDASH_DESKTOP_API_ORIGIN
      || options.defaultApiOrigin
      || 'http://127.0.0.1:3001',
  );
  let apiSidecar = normalizeOptionalValue(
    env.AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR
      || env.AGENTDASH_DESKTOP_API_SIDECAR
      || options.defaultApiSidecar,
  );
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
      apiOrigin = normalizeOrigin(arg.slice('--api-origin='.length));
      continue;
    }
    if (arg === '--api-origin') {
      apiOrigin = normalizeOrigin(readNextValue(args, ++index, arg));
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

  return {
    apiMode,
    apiOrigin,
    apiSidecar,
    help,
    passthrough,
    root,
    sccacheDir,
    sccacheMode,
    tauriConfigPath: options.tauriConfigPath,
  };
}

function printHelp(options) {
  const defaultMode = options.defaultApiMode || 'builtin';
  const defaultOrigin = options.defaultApiOrigin || 'http://127.0.0.1:3001';
  console.log('用法: node ./scripts/desktop-build.js [build-options] [...tauri-build-options]');
  console.log('');
  console.log('AgentDash 桌面端构建入口。');
  console.log('');
  console.log('Build options:');
  console.log(`  --api-mode <builtin|external|sidecar>  桌面壳默认 API 模式，默认 ${defaultMode}`);
  console.log(`  --api-origin <url>                    API origin，默认 ${defaultOrigin}`);
  console.log('  --api-sidecar <command>               sidecar 模式下启动的 API 可执行文件');
  console.log('  --sccache                             要求使用 sccache');
  console.log('  --no-sccache                          关闭 RUSTC_WRAPPER');
  console.log('  --sccache-dir <path>                  指定 SCCACHE_DIR');
  console.log('  -h, --help                            显示帮助');
  console.log('');
  console.log('其他参数会原样传给 pnpm exec tauri build。');
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

function normalizeOrigin(value) {
  const trimmed = String(value || '').trim().replace(/\/+$/, '');
  if (!trimmed) {
    throw new Error('--api-origin 不能为空');
  }
  return trimmed;
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

function resolvePnpmCommand() {
  return process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
}

function formatSccacheDescription(sccachePath, cacheDir) {
  return `sccache (${sccachePath})${formatCacheDirSuffix(cacheDir)}`;
}

function formatCacheDirSuffix(cacheDir) {
  const normalized = normalizeOptionalValue(cacheDir);
  return normalized ? `，SCCACHE_DIR=${normalized}` : '';
}
