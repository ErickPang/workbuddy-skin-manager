const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { checkTheme, clearTheme, injectTheme } = require('./cdp-client');

const ENGINE_RUNTIME_FILES = ['workbuddy-theme.js', 'cdp-client.js'];
const MAX_JSON_BYTES = 256 * 1024;
const MAX_IMAGE_BYTES = 8 * 1024 * 1024;
const PALETTE_KEYS = [
  'background', 'panel', 'panelAlt', 'text', 'muted', 'accent',
  'accentText', 'border', 'hover', 'active', 'subtle',
];

function readThemeFile(themeDir, relativePath, label, maxBytes) {
  const rootMetadata = fs.lstatSync(themeDir);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) {
    throw new Error('主题目录必须是普通目录');
  }
  const root = fs.realpathSync(themeDir);
  const candidate = path.resolve(root, relativePath);
  const resolved = fs.realpathSync(candidate);
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    throw new Error(`${label}超出主题目录`);
  }
  let current = root;
  for (const component of path.relative(root, candidate).split(path.sep)) {
    if (!component || component === '.' || component === '..') {
      throw new Error(`${label}路径无效`);
    }
    current = path.join(current, component);
    if (fs.lstatSync(current).isSymbolicLink()) {
      throw new Error(`${label}路径不能包含符号链接`);
    }
  }
  const metadata = fs.lstatSync(candidate);
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    throw new Error(`${label}必须是普通文件`);
  }
  if (metadata.size > maxBytes) {
    throw new Error(`${label}超过大小限制`);
  }
  return { bytes: fs.readFileSync(candidate), path: resolved };
}

function parseJson(bytes, label) {
  try {
    return JSON.parse(bytes.toString('utf8'));
  } catch (error) {
    throw new Error(`${label}格式错误: ${error.message}`);
  }
}

function validateImagePath(value) {
  if (typeof value !== 'string' || value.includes('\\')) {
    throw new Error('主题背景路径无效');
  }
  const parts = value.split('/');
  if (parts[0] !== 'assets' || parts.some(part => !part || part === '.' || part === '..')) {
    throw new Error('主题背景必须位于 assets 目录');
  }
  if (!['.png', '.jpg', '.jpeg', '.webp'].includes(path.extname(value).toLowerCase())) {
    throw new Error('主题背景格式不受支持');
  }
}

function validateThemeConfig(config) {
  if (!config || typeof config !== 'object' || Array.isArray(config)) {
    throw new Error('theme.json 内容无效');
  }
  if (!config.palette || typeof config.palette !== 'object' || Array.isArray(config.palette)) {
    throw new Error('主题 palette 无效');
  }
  for (const key of PALETTE_KEYS) {
    if (typeof config.palette[key] !== 'string' || !/^#[0-9a-fA-F]{6}$/.test(config.palette[key])) {
      throw new Error(`主题颜色 ${key} 无效`);
    }
  }
  if (!config.background || typeof config.background !== 'object' || Array.isArray(config.background)) {
    throw new Error('主题 background 无效');
  }
  validateImagePath(config.background.image);
  if (!['cover', 'contain'].includes(config.background.size)) {
    throw new Error('主题背景 size 无效');
  }
  if (typeof config.background.position !== 'string'
      || !config.background.position.trim()
      || config.background.position.length > 40) {
    throw new Error('主题背景 position 无效');
  }
}

function loadTheme(themeDir) {
  const manifestFile = readThemeFile(themeDir, 'manifest.json', 'manifest.json', MAX_JSON_BYTES);
  const configFile = readThemeFile(themeDir, 'theme.json', 'theme.json', MAX_JSON_BYTES);
  const manifest = parseJson(manifestFile.bytes, 'manifest.json');
  const config = parseJson(configFile.bytes, 'theme.json');
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)
      || typeof manifest.name !== 'string' || !manifest.name.trim() || manifest.name.length > 80) {
    throw new Error('manifest.json 的 name 无效');
  }
  if (manifest.description !== undefined
      && (typeof manifest.description !== 'string' || manifest.description.length > 240)) {
    throw new Error('manifest.json 的 description 无效');
  }
  validateThemeConfig(config);
  const backgroundFile = readThemeFile(
    themeDir,
    config.background.image,
    '主题背景',
    MAX_IMAGE_BYTES,
  );
  const runtimeHash = crypto.createHash('sha256')
    .update(manifestFile.bytes)
    .update(configFile.bytes)
    .update(backgroundFile.bytes);
  for (const file of ENGINE_RUNTIME_FILES) {
    runtimeHash.update(fs.readFileSync(path.join(__dirname, file)));
  }
  const runtimeKey = runtimeHash.digest('hex');
  return {
    name: manifest.name,
    runtimeKey,
    description: manifest.description || '',
    workbuddy: { palette: config.palette },
    overlay: {
      background: backgroundFile.path,
      backgroundBytes: backgroundFile.bytes,
      backgroundPosition: config.background.position,
      backgroundSize: config.background.size,
    },
  };
}

async function main() {
  const [command, themeDir, rawPort] = process.argv.slice(2);
  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error(`Invalid CDP port: ${rawPort}`);
  }

  if (command === 'restore') {
    await clearTheme({ port, timeoutMs: 5000 });
    console.log(JSON.stringify({ restored: true }));
    return;
  }

  if (!themeDir) throw new Error('Theme directory is required');
  const theme = loadTheme(themeDir);
  if (command === 'apply') {
    const result = await injectTheme(theme, { port, timeoutMs: 30000, verificationTimeoutMs: 8000 });
    console.log(JSON.stringify({ applied: true, verification: result.verification }));
    return;
  }
  if (command === 'check') {
    const active = await checkTheme(theme.runtimeKey, { port, timeoutMs: 3000, requireBackground: true });
    console.log(JSON.stringify({ active }));
    process.exitCode = active ? 0 : 2;
    return;
  }
  throw new Error(`Unknown command: ${command}`);
}

if (require.main === module) {
  main().catch(error => {
    console.error(error && error.message ? error.message : String(error));
    process.exit(1);
  });
}

module.exports = { ENGINE_RUNTIME_FILES, loadTheme };
