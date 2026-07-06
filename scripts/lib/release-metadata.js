import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../..');
const RELEASE_SCHEMA_VERSION = 1;
const DEFAULT_PRODUCT = 'AgentDash';
const DEFAULT_PRODUCT_SLUG = 'agentdash';
const DEFAULT_CHANNEL = 'stable';
const DEFAULT_PLATFORM = 'windows';
const DEFAULT_ARCH = 'x86_64';
const DEFAULT_OBJECT_KEY_PREFIX = 'desktop';
const WINDOWS_UPDATER_SUFFIXES = ['.nsis.zip', '.msi.zip'];

export async function main(argv, options = {}) {
  const root = path.resolve(options.root || REPO_ROOT);
  const args = parseArgs(argv);
  const manifest = buildReleaseMetadata({ root, args, env: options.env || process.env });

  if (args.desktopReleaseDir) {
    manifest.desktop_release = generateDesktopReleaseDirectory({
      root,
      releaseDir: args.desktopReleaseDir,
      artifactsDir: args.desktopArtifactsDir,
      metadata: manifest,
      channel: args.channel,
      platform: args.platform,
      arch: args.arch,
      objectKeyPrefix: args.objectKeyPrefix,
      productSlug: DEFAULT_PRODUCT_SLUG,
    });
  }

  const output = `${JSON.stringify(manifest, null, 2)}\n`;
  if (args.out) {
    const outPath = path.resolve(root, args.out);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, output, 'utf8');
  } else {
    process.stdout.write(output);
  }
}

export function buildReleaseMetadata({ root = REPO_ROOT, args = {}, env = process.env } = {}) {
  const packageJson = readJson(path.join(root, 'package.json'));
  const cargoMetadata = readCargoMetadata(root);
  const gitSha = readGitSha(root);
  const buildTime = normalizeOptionalValue(env.AGENTDASH_BUILD_TIME) || new Date().toISOString();
  const releaseNotes =
    normalizeOptionalValue(env.AGENTDASH_RELEASE_NOTES) || `${DEFAULT_PRODUCT} ${packageJson.version}`;
  const imageRepository = normalizeOptionalValue(env.AGENTDASH_IMAGE_REPOSITORY) || 'agentdash-cloud';
  const channel = args.channel || DEFAULT_CHANNEL;
  const platform = args.platform || DEFAULT_PLATFORM;
  const arch = args.arch || DEFAULT_ARCH;
  const platformKey = `${platform}-${arch}`;

  const workspaceVersions = collectWorkspaceVersions(cargoMetadata);
  if (!workspaceVersions.includes(packageJson.version)) {
    throw new Error(
      `根 package.json version (${packageJson.version}) 未出现在 Cargo workspace package versions: ${workspaceVersions.join(', ')}`,
    );
  }

  return {
    product: DEFAULT_PRODUCT,
    version: packageJson.version,
    git_sha: gitSha,
    build_time: buildTime,
    published_at: buildTime,
    release_notes: releaseNotes,
    package_manager: packageJson.packageManager,
    cargo_versions: workspaceVersions,
    channel,
    release_manifest_schema_version: RELEASE_SCHEMA_VERSION,
    artifacts: {
      server_binary: 'agentdash-server',
      cloud_image: `${imageRepository}:${packageJson.version}`,
      web_dist: 'packages/app-web/dist',
      desktop_installer: `AgentDash_${packageJson.version}_x64-setup.exe`,
    },
    platforms: {
      [platformKey]: {
        platform,
        arch,
        installer: {
          kind: 'manual_installer',
          expected_pattern: 'target/release/bundle/nsis/*.exe',
        },
        updater: {
          kind: 'tauri_updater',
          expected_patterns: [
            'target/release/bundle/nsis/*.nsis.zip',
            'target/release/bundle/nsis/*.msi.zip',
          ],
          signature_pattern: '<updater_artifact>.sig',
        },
      },
    },
  };
}

export function generateDesktopReleaseDirectory(options) {
  const root = path.resolve(options.root || REPO_ROOT);
  const releaseDir = path.resolve(root, options.releaseDir);
  const artifactsDir = path.resolve(root, options.artifactsDir || 'target/release/bundle');
  const channel = options.channel || DEFAULT_CHANNEL;
  const platform = options.platform || DEFAULT_PLATFORM;
  const arch = options.arch || DEFAULT_ARCH;
  const platformKey = `${platform}-${arch}`;
  const objectKeyPrefix = normalizeObjectKeyPrefix(options.objectKeyPrefix || DEFAULT_OBJECT_KEY_PREFIX);
  const productSlug = options.productSlug || DEFAULT_PRODUCT_SLUG;
  const product = options.metadata.product || DEFAULT_PRODUCT;
  const version = options.metadata.version;
  const publishedAt = options.metadata.published_at || options.metadata.build_time;
  const releaseNotes = options.metadata.release_notes || `${product} ${version}`;

  if (platform !== DEFAULT_PLATFORM || arch !== DEFAULT_ARCH) {
    throw new Error(`当前 release metadata 只支持 ${DEFAULT_PLATFORM}-${DEFAULT_ARCH} fixture 发现，收到 ${platform}-${arch}`);
  }

  const artifacts = discoverWindowsDesktopArtifacts(artifactsDir);
  const versionRoot = path.posix.join('releases', productSlug, version);
  const platformRoot = path.posix.join(versionRoot, platform, arch);
  const releaseManifestRelative = path.posix.join(versionRoot, 'release.json');
  const releaseManifestShaRelative = `${releaseManifestRelative}.sha256`;
  const latestRelative = path.posix.join('channels', channel, 'latest.json');
  const uploadPlanRelative = 'upload-plan.json';

  fs.mkdirSync(releaseDir, { recursive: true });

  const installer = copyArtifactWithSha({
    source: artifacts.installer,
    releaseDir,
    relativeDir: platformRoot,
    objectKeyPrefix,
  });
  const updater = copyArtifactWithSha({
    source: artifacts.updater,
    releaseDir,
    relativeDir: platformRoot,
    objectKeyPrefix,
  });
  const signature = copySignature({
    source: artifacts.signature,
    releaseDir,
    relativeDir: platformRoot,
    objectKeyPrefix,
  });

  const releaseManifest = {
    schema_version: RELEASE_SCHEMA_VERSION,
    product,
    version,
    git_sha: options.metadata.git_sha,
    build_time: options.metadata.build_time,
    published_at: publishedAt,
    release_notes: releaseNotes,
    channel,
    platforms: {
      [platformKey]: {
        platform,
        arch,
        installer: {
          kind: 'manual_installer',
          file: installer.file,
          object_key: installer.object_key,
          sha256: installer.sha256,
          sha256_file: installer.sha256_file,
          public_url: null,
        },
        updater: {
          kind: 'tauri_updater',
          file: updater.file,
          object_key: updater.object_key,
          sha256: updater.sha256,
          sha256_file: updater.sha256_file,
          signature: signature.value,
          signature_file: signature.file,
          signature_object_key: signature.object_key,
          public_url: null,
        },
      },
    },
  };
  writeJsonFile(path.join(releaseDir, fromPosixPath(releaseManifestRelative)), releaseManifest);
  const releaseManifestSha = writeSha256File(path.join(releaseDir, fromPosixPath(releaseManifestRelative)));
  writeTextFile(path.join(releaseDir, fromPosixPath(releaseManifestShaRelative)), `${releaseManifestSha}  release.json\n`);

  const latestManifest = {
    schema_version: RELEASE_SCHEMA_VERSION,
    product,
    channel,
    version,
    git_sha: options.metadata.git_sha,
    build_time: options.metadata.build_time,
    published_at: publishedAt,
    release_notes: releaseNotes,
    release_manifest: {
      file: releaseManifestRelative,
      object_key: objectKeyFor(objectKeyPrefix, releaseManifestRelative),
      sha256: releaseManifestSha,
      sha256_file: releaseManifestShaRelative,
      public_url: null,
    },
    platforms: releaseManifest.platforms,
  };
  writeJsonFile(path.join(releaseDir, fromPosixPath(latestRelative)), latestManifest);

  const uploads = [
    uploadEntry(releaseManifestRelative, objectKeyPrefix, 'application/json', true),
    uploadEntry(releaseManifestShaRelative, objectKeyPrefix, 'text/plain; charset=utf-8', true),
    artifactUploadEntry(installer, 'application/vnd.microsoft.portable-executable'),
    uploadEntry(installer.sha256_file, objectKeyPrefix, 'text/plain; charset=utf-8', true),
    artifactUploadEntry(updater, 'application/zip'),
    uploadEntry(updater.sha256_file, objectKeyPrefix, 'text/plain; charset=utf-8', true),
    uploadEntry(signature.file, objectKeyPrefix, 'text/plain; charset=utf-8', true),
    uploadEntry(latestRelative, objectKeyPrefix, 'application/json', false),
  ];
  const uploadPlan = {
    schema_version: RELEASE_SCHEMA_VERSION,
    product,
    version,
    channel,
    root_dir: pathToPosix(path.relative(root, releaseDir)) || '.',
    object_storage: {
      contract: 's3-compatible',
      object_key_prefix: objectKeyPrefix,
      public_base_url_env: 'AGENTDASH_DESKTOP_RELEASE_PUBLIC_BASE_URL',
      private_mapping_owner: 'private-deployment',
    },
    uploads,
  };
  writeJsonFile(path.join(releaseDir, uploadPlanRelative), uploadPlan);

  return {
    release_dir: pathToPosix(path.relative(root, releaseDir)) || '.',
    release_manifest: releaseManifestRelative,
    stable_latest_manifest: latestRelative,
    upload_plan: uploadPlanRelative,
    platforms: Object.keys(releaseManifest.platforms),
  };
}

export function discoverWindowsDesktopArtifacts(artifactsDir) {
  const files = listFilesRecursive(artifactsDir);
  const installerCandidates = files.filter((file) => {
    const lower = file.toLowerCase();
    return lower.endsWith('.exe') && !lower.endsWith('.sig');
  });
  const updaterCandidates = files.filter((file) => {
    const lower = file.toLowerCase();
    return WINDOWS_UPDATER_SUFFIXES.some((suffix) => lower.endsWith(suffix));
  });

  const installer = requireSingleArtifact({
    candidates: installerCandidates,
    artifactsDir,
    label: 'Windows NSIS installer',
    expected: '*.exe',
  });
  const updater = requireSingleArtifact({
    candidates: updaterCandidates,
    artifactsDir,
    label: 'Tauri updater artifact',
    expected: '*.nsis.zip 或 *.msi.zip',
  });
  const signature = `${updater}.sig`;
  if (!fs.existsSync(signature)) {
    throw new Error(
      `未发现 Tauri updater signature: ${signature}；期望 signature 文件与 updater artifact 同名并追加 .sig`,
    );
  }

  return {
    installer,
    updater,
    signature,
  };
}

export function parseArgs(values) {
  const result = {
    out: null,
    desktopReleaseDir: null,
    desktopArtifactsDir: 'target/release/bundle',
    channel: DEFAULT_CHANNEL,
    platform: DEFAULT_PLATFORM,
    arch: DEFAULT_ARCH,
    objectKeyPrefix: DEFAULT_OBJECT_KEY_PREFIX,
  };
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value === '--out') {
      result.out = readNextValue(values, ++index, value);
      continue;
    }
    if (value.startsWith('--out=')) {
      result.out = requireNonEmpty(value.slice('--out='.length), '--out');
      continue;
    }
    if (value === '--desktop-release-dir' || value === '--release-dir') {
      result.desktopReleaseDir = readNextValue(values, ++index, value);
      continue;
    }
    if (value.startsWith('--desktop-release-dir=')) {
      result.desktopReleaseDir = requireNonEmpty(value.slice('--desktop-release-dir='.length), '--desktop-release-dir');
      continue;
    }
    if (value.startsWith('--release-dir=')) {
      result.desktopReleaseDir = requireNonEmpty(value.slice('--release-dir='.length), '--release-dir');
      continue;
    }
    if (value === '--desktop-artifacts-dir') {
      result.desktopArtifactsDir = readNextValue(values, ++index, value);
      continue;
    }
    if (value.startsWith('--desktop-artifacts-dir=')) {
      result.desktopArtifactsDir = requireNonEmpty(value.slice('--desktop-artifacts-dir='.length), '--desktop-artifacts-dir');
      continue;
    }
    if (value === '--channel') {
      result.channel = normalizeChannel(readNextValue(values, ++index, value));
      continue;
    }
    if (value.startsWith('--channel=')) {
      result.channel = normalizeChannel(value.slice('--channel='.length));
      continue;
    }
    if (value === '--platform') {
      result.platform = normalizeToken(readNextValue(values, ++index, value), value);
      continue;
    }
    if (value.startsWith('--platform=')) {
      result.platform = normalizeToken(value.slice('--platform='.length), '--platform');
      continue;
    }
    if (value === '--arch') {
      result.arch = normalizeToken(readNextValue(values, ++index, value), value);
      continue;
    }
    if (value.startsWith('--arch=')) {
      result.arch = normalizeToken(value.slice('--arch='.length), '--arch');
      continue;
    }
    if (value === '--object-key-prefix') {
      result.objectKeyPrefix = normalizeObjectKeyPrefix(readNextValue(values, ++index, value));
      continue;
    }
    if (value.startsWith('--object-key-prefix=')) {
      result.objectKeyPrefix = normalizeObjectKeyPrefix(value.slice('--object-key-prefix='.length));
      continue;
    }
    throw new Error(`未知参数: ${value}`);
  }
  return result;
}

function copyArtifactWithSha({ source, releaseDir, relativeDir, objectKeyPrefix }) {
  const basename = path.basename(source);
  const relativeFile = path.posix.join(relativeDir, basename);
  const target = path.join(releaseDir, fromPosixPath(relativeFile));
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
  const sha256 = sha256File(target);
  const sha256FileRelative = `${relativeFile}.sha256`;
  writeTextFile(path.join(releaseDir, fromPosixPath(sha256FileRelative)), `${sha256}  ${basename}\n`);
  return {
    file: relativeFile,
    object_key: objectKeyFor(objectKeyPrefix, relativeFile),
    sha256,
    sha256_file: sha256FileRelative,
  };
}

function copySignature({ source, releaseDir, relativeDir, objectKeyPrefix }) {
  const basename = path.basename(source);
  const relativeFile = path.posix.join(relativeDir, basename);
  const target = path.join(releaseDir, fromPosixPath(relativeFile));
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
  return {
    file: relativeFile,
    object_key: objectKeyFor(objectKeyPrefix, relativeFile),
    value: fs.readFileSync(source, 'utf8').trim(),
  };
}

function uploadEntry(localPath, objectKeyPrefix, contentType, immutable) {
  return {
    local_path: localPath,
    object_key: objectKeyFor(objectKeyPrefix, localPath),
    content_type: contentType,
    immutable,
  };
}

function artifactUploadEntry(artifact, contentType) {
  return {
    local_path: artifact.file,
    object_key: artifact.object_key,
    content_type: contentType,
    immutable: true,
  };
}

function requireSingleArtifact({ candidates, artifactsDir, label, expected }) {
  if (candidates.length === 1) {
    return candidates[0];
  }
  if (candidates.length === 0) {
    throw new Error(
      `未发现 ${label}: 期望在 ${artifactsDir} 下唯一匹配 ${expected}；请先运行对应桌面构建或在测试中提供 fixture`,
    );
  }
  throw new Error(
    `${label} 匹配到多个候选，无法稳定定位: ${candidates.map((file) => path.relative(artifactsDir, file)).join(', ')}`,
  );
}

function listFilesRecursive(dir) {
  if (!fs.existsSync(dir)) {
    return [];
  }
  const result = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      result.push(...listFilesRecursive(file));
      continue;
    }
    if (entry.isFile()) {
      result.push(file);
    }
  }
  return result.sort();
}

function writeJsonFile(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function writeTextFile(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, value, 'utf8');
}

function writeSha256File(filePath) {
  return sha256File(filePath);
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function normalizeOptionalValue(value) {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function normalizeChannel(value) {
  const channel = normalizeToken(value, '--channel');
  if (channel !== DEFAULT_CHANNEL) {
    throw new Error(`当前桌面 release channel 固定为 ${DEFAULT_CHANNEL}，收到 ${channel}`);
  }
  return channel;
}

function normalizeToken(value, flagName) {
  const normalized = requireNonEmpty(String(value || '').trim().toLowerCase(), flagName);
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(normalized)) {
    throw new Error(`${flagName} 只能包含小写字母、数字、下划线或短横线`);
  }
  return normalized;
}

function normalizeObjectKeyPrefix(value) {
  const trimmed = requireNonEmpty(String(value || '').trim(), '--object-key-prefix')
    .replace(/^\/+/, '')
    .replace(/\/+$/, '');
  if (!trimmed || trimmed.includes('://') || trimmed.includes('\\')) {
    throw new Error('--object-key-prefix 必须是对象 key 前缀，不能是 URL 或 Windows 路径');
  }
  return trimmed;
}

function readNextValue(values, index, flagName) {
  const value = values[index];
  if (!value) {
    throw new Error(`${flagName} 缺少取值`);
  }
  return requireNonEmpty(value, flagName);
}

function requireNonEmpty(value, flagName) {
  if (!value) {
    throw new Error(`${flagName} 不能为空`);
  }
  return value;
}

function readCargoMetadata(root) {
  const stdout = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  });
  return JSON.parse(stdout);
}

function readGitSha(root) {
  return execFileSync('git', ['rev-parse', '--short=12', 'HEAD'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
}

function collectWorkspaceVersions(metadata) {
  const memberIds = new Set(metadata.workspace_members);
  const versions = new Set();
  for (const pkg of metadata.packages) {
    if (memberIds.has(pkg.id)) {
      versions.add(pkg.version);
    }
  }
  return [...versions].sort();
}

function objectKeyFor(prefix, localPath) {
  return path.posix.join(prefix, localPath);
}

function pathToPosix(value) {
  return value.split(path.sep).join('/');
}

function fromPosixPath(value) {
  return value.split('/').join(path.sep);
}
