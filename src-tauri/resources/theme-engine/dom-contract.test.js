const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const { JSDOM } = require('jsdom');

const { buildInstallerExpression, STYLE_ID } = require('./cdp-client');
const { SELECTORS } = require('./workbuddy-theme');

for (const version of ['5.2', '5.3']) {
  test(`keeps the WorkBuddy ${version} DOM selector contract`, async () => {
    const html = fs.readFileSync(
      path.join(__dirname, 'fixtures', `workbuddy-${version}.html`),
      'utf8'
    );
    const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'https://workbuddy.local/' });
    const { window } = dom;
    window.URL.createObjectURL = () => 'blob:wbskin-fixture';
    window.URL.revokeObjectURL = () => {};

    for (const [name, selector] of Object.entries(SELECTORS)) {
      assert.ok(window.document.querySelector(selector), `${version} fixture missing ${name}: ${selector}`);
    }

    const expression = buildInstallerExpression(
      { name: `fixture-${version}`, runtimeKey: `fixture-${version}`, overlay: {} },
      ':root { --wbskin-fixture: ready; }',
      'data:image/png;base64,AAAA'
    );
    await window.eval(expression);

    assert.ok(window.document.documentElement.classList.contains('wbskin-active'));
    assert.ok(window.document.documentElement.classList.contains('wbskin-has-art'));
    assert.equal(window.document.getElementById(STYLE_ID)?.textContent, ':root { --wbskin-fixture: ready; }');
    assert.equal(window.__WBSKIN_STATE__.key, `fixture-${version}`);
    assert.match(window.document.documentElement.style.getPropertyValue('--wbskin-art'), /blob:wbskin-fixture/);
    window.close();
  });
}
