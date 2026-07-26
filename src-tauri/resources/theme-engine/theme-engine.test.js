const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const { loadTheme } = require('./runner');
const { buildInstallerExpression, buildVerificationExpression, CDPClient } = require('./cdp-client');
const { SELECTORS, buildPalette, buildThemeCSS, inferColorScheme } = require('./workbuddy-theme');

const palette = {
  background: '#101010',
  panel: '#202020',
  panelAlt: '#303030',
  text: '#f0f0f0',
  muted: '#a0a0a0',
  accent: '#cc3366',
  accentText: '#ffffff',
  border: '#404040',
  hover: '#505050',
  active: '#606060',
  subtle: '#181818',
};

test('builds WorkBuddy tokens and scoped CSS from a validated palette', () => {
  const theme = { workbuddy: { palette } };
  const built = buildPalette(theme);
  const css = buildThemeCSS(theme);

  assert.equal(built.tokens['--wb-brand-primary'], palette.accent);
  assert.match(css, /html\.wbskin-active/);
  assert.match(css, /--wb-brand-primary: #cc3366 !important/);
  assert.match(css, /\.wbskin-has-art \.teams-container/);
  assert.match(css, /background-image: var\(--wbskin-art\)/);
  assert.match(css, /\.workbuddy-topbar/);
  assert.match(css, /backdrop-filter: blur\(18px\)/);
  assert.match(css, /conversation-list-tab-button\.active/);
  assert.match(css, /conversation-list-tab-button-box\.active/);
  assert.doesNotMatch(css, /linear-gradient/);
});

test('uses a native color scheme that matches the theme background', () => {
  assert.equal(inferColorScheme('#101010'), 'dark');
  assert.equal(inferColorScheme('#f5eee8'), 'light');
});

test('changes the runtime key when injected theme content changes', (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'wbskin-engine-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  fs.mkdirSync(path.join(directory, 'assets'));
  fs.writeFileSync(path.join(directory, 'manifest.json'), JSON.stringify({ name: 'Fixture' }));
  fs.writeFileSync(path.join(directory, 'theme.json'), JSON.stringify({
    palette,
    background: { image: 'assets/background.png', position: 'center', size: 'cover' },
  }));
  const background = path.join(directory, 'assets/background.png');
  fs.writeFileSync(background, Buffer.from('first image'));
  const first = loadTheme(directory);
  fs.writeFileSync(background, Buffer.from('second image'));
  const second = loadTheme(directory);

  assert.notEqual(first.runtimeKey, second.runtimeKey);
  assert.equal(first.overlay.background, background);
});

test('keeps the required WorkBuddy selector contract in verification', () => {
  const expression = buildVerificationExpression({ workbuddy: { palette } }, null);

  for (const selector of Object.values(SELECTORS)) {
    const serializedSelector = JSON.stringify(selector).slice(1, -1);
    assert.ok(expression.includes(serializedSelector), `missing verification selector: ${selector}`);
  }
});

test('installs large background data through a short Blob URL', () => {
  const expression = buildInstallerExpression(
    { name: 'large-background', runtimeKey: 'fixture', overlay: {} },
    ':root { --test: red; }',
    'data:image/png;base64,AAAA'
  );

  assert.match(expression, /atob\(dataUri\.slice/);
  assert.doesNotMatch(expression, /fetch\(dataUri\)/);
  assert.match(expression, /URL\.createObjectURL\(blob\)/);
  assert.match(expression, /setProperty\('--wbskin-art', 'url\(' \+ JSON\.stringify\(artUrl\)/);
  assert.match(expression, /URL\.revokeObjectURL/);
});

test('verifies the computed shell background image', () => {
  const expression = buildVerificationExpression(
    { workbuddy: { palette } },
    'data:image/png;base64,AAAA'
  );

  assert.match(expression, /backgroundImage !== 'none'/);
});

test('accepts only an explicit loopback CDP endpoint', () => {
  assert.doesNotThrow(() => new CDPClient('127.0.0.1', 49152));
  assert.throws(() => new CDPClient('localhost', 49152), /无效的本机 CDP 地址/);
  assert.throws(() => new CDPClient('127.0.0.1', 922), /无效的本机 CDP 地址/);
});

test('rejects a CDP WebSocket URL that changes the explicit host', async () => {
  const client = new CDPClient('127.0.0.1', 49152);
  await assert.rejects(
    () => client.connect({ webSocketDebuggerUrl: 'ws://localhost:49152/devtools/page/fixture' }),
    /拒绝非本机 CDP WebSocket/
  );
});
