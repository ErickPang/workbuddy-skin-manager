const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { checkTheme, clearTheme, injectTheme } = require('./cdp-client');

function loadTheme(themeDir) {
  const manifestBytes = fs.readFileSync(path.join(themeDir, 'manifest.json'));
  const configBytes = fs.readFileSync(path.join(themeDir, 'theme.json'));
  const manifest = JSON.parse(manifestBytes.toString('utf8'));
  const config = JSON.parse(configBytes.toString('utf8'));
  const backgroundPath = path.join(themeDir, config.background.image);
  const runtimeKey = crypto.createHash('sha256')
    .update(manifestBytes)
    .update(configBytes)
    .update(fs.readFileSync(backgroundPath))
    .digest('hex');
  return {
    name: manifest.name,
    runtimeKey,
    description: manifest.description || '',
    workbuddy: { palette: config.palette },
    overlay: {
      background: backgroundPath,
      backgroundPosition: config.background.position || 'right center',
      backgroundSize: config.background.size || 'cover',
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

module.exports = { loadTheme };
