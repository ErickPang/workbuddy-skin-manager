/**
 * WorkBuddy CDP client and runtime theme injector.
 */

const fs = require('fs');
const path = require('path');
const { buildPalette, buildThemeCSS } = require('./workbuddy-theme');

const DEFAULT_CDP_HOST = '127.0.0.1';
const STYLE_ID = 'wbskin-theme-style';
const BACKGROUND_ID = 'wbskin-background';

let _WebSocket = null;
function getWebSocket() {
  if (_WebSocket) return _WebSocket;
  if (typeof globalThis.WebSocket === 'function') {
    _WebSocket = globalThis.WebSocket;
    return _WebSocket;
  }
  throw new Error('当前 WorkBuddy Node runtime 不支持 WebSocket');
}

function addSocketListener(socket, event, handler, options) {
  if (typeof socket.addEventListener === 'function') {
    socket.addEventListener(event, handler, options);
  } else {
    socket.once(event, handler);
  }
}

function socketMessageData(event) {
  const data = event && typeof event === 'object' && 'data' in event ? event.data : event;
  return Buffer.isBuffer(data) ? data.toString() : String(data);
}

class CDPClient {
  constructor(host, port) {
    if (host !== DEFAULT_CDP_HOST || !Number.isInteger(port) || port < 1024 || port > 65535) {
      throw new Error(`无效的本机 CDP 地址: ${host}:${port}`);
    }
    this.host = host;
    this.port = port;
    this.ws = null;
    this.msgId = 0;
    this.pending = new Map();
  }

  async getTargets() {
    const response = await fetch(`http://${this.host}:${this.port}/json/list`, {
      signal: AbortSignal.timeout(3000),
    });
    if (!response.ok) throw new Error(`CDP 连接失败: HTTP ${response.status}`);
    const targets = await response.json();
    if (!Array.isArray(targets)) throw new Error('CDP target 列表格式无效');
    return targets.filter(target =>
      (target.type === 'page' || target.type === 'webview') && target.webSocketDebuggerUrl
    );
  }

  async connect(target) {
    this.disconnect();
    const wsUrl = new URL(target.webSocketDebuggerUrl);
    if (wsUrl.protocol !== 'ws:' || Number(wsUrl.port) !== Number(this.port)) {
      throw new Error(`拒绝非预期 CDP WebSocket: ${wsUrl.href}`);
    }
    if (!['127.0.0.1', 'localhost', '[::1]'].includes(wsUrl.hostname)) {
      throw new Error(`拒绝非本机 CDP WebSocket: ${wsUrl.href}`);
    }

    const WebSocket = getWebSocket();
    const socket = new WebSocket(wsUrl.href);
    this.ws = socket;
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        socket.close();
        reject(new Error('WebSocket 连接超时'));
      }, 5000);
      addSocketListener(socket, 'open', () => {
        clearTimeout(timer);
        resolve();
      }, { once: true });
      addSocketListener(socket, 'error', () => {
        clearTimeout(timer);
        reject(new Error('WebSocket 连接失败'));
      }, { once: true });
    });

    const onMessage = event => {
      let message;
      try {
        message = JSON.parse(socketMessageData(event));
      } catch {
        if (this.ws === socket) {
          this.ws = null;
          socket.close();
          this.rejectPending(new Error('CDP 返回了无效消息'));
        }
        return;
      }
      if (!message.id || !this.pending.has(message.id)) return;
      const waiter = this.pending.get(message.id);
      clearTimeout(waiter.timer);
      this.pending.delete(message.id);
      if (message.error) waiter.reject(new Error(`${message.error.message} (${message.error.code})`));
      else waiter.resolve(message.result);
    };
    const onClose = () => {
      if (this.ws !== socket) return;
      this.ws = null;
      this.rejectPending(new Error('CDP WebSocket 已关闭'));
    };
    if (typeof socket.addEventListener === 'function') {
      socket.addEventListener('message', onMessage);
      socket.addEventListener('close', onClose, { once: true });
    } else {
      socket.on('message', onMessage);
      socket.once('close', onClose);
    }
  }

  sendCommand(method, params = {}) {
    if (!this.ws) return Promise.reject(new Error('CDP WebSocket 未连接'));
    const id = ++this.msgId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP 命令超时: ${method}`));
      }, 10000);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.ws.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  async evaluate(expression) {
    const response = await this.sendCommand('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (response.exceptionDetails) {
      const detail = response.exceptionDetails.exception?.description || response.exceptionDetails.text;
      throw new Error(`页面脚本执行失败: ${detail}`);
    }
    return response.result ? response.result.value : undefined;
  }

  async probe() {
    return this.evaluate(`(() => ({
      title: document.title,
      href: location.href,
      workbuddy: Boolean(
        document.querySelector('.teams-container') ||
        document.querySelector('.sidebar-next') ||
        document.querySelector('.detail-panel-container') ||
        document.querySelector('[data-view-id="sidebar"]')
      )
    }))()`);
  }

  async clearAll() {
    return this.evaluate(`(() => {
      document.documentElement.classList.remove('wbskin-active');
      document.documentElement.classList.remove('wbskin-has-art');
      document.documentElement.classList.remove('wbskin-proof');
      document.documentElement.style.removeProperty('--wbskin-art');
      document.documentElement.style.removeProperty('--wbskin-art-position');
      document.documentElement.style.removeProperty('--wbskin-art-size');
      if (window.__WBSKIN_ART_URL__) URL.revokeObjectURL(window.__WBSKIN_ART_URL__);
      delete window.__WBSKIN_ART_URL__;
      document.getElementById(${JSON.stringify(STYLE_ID)})?.remove();
      document.getElementById(${JSON.stringify(BACKGROUND_ID)})?.remove();
      document.getElementById('wbskin-proof-style')?.remove();
      delete window.__WBSKIN_STATE__;
      return true;
    })()`);
  }

  disconnect() {
    const socket = this.ws;
    this.ws = null;
    if (socket) socket.close();
    this.rejectPending(new Error('CDP WebSocket 已断开'));
  }

  rejectPending(error) {
    for (const waiter of this.pending.values()) {
      clearTimeout(waiter.timer);
      waiter.reject(error);
    }
    this.pending.clear();
  }
}

async function connectToWorkBuddy(client, timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const targets = await client.getTargets();
      for (const target of targets) {
        try {
          await client.connect(target);
          const probe = await client.probe();
          if (probe && probe.workbuddy) return { target, probe };
          client.disconnect();
        } catch (error) {
          lastError = error;
          client.disconnect();
        }
      }
      if (!targets.length) lastError = new Error('没有 page/webview target');
    } catch (error) {
      lastError = error;
    }
    await new Promise(resolve => setTimeout(resolve, 400));
  }
  throw new Error(`未找到 WorkBuddy 主界面: ${lastError ? lastError.message : '等待超时'}`);
}

function resolveBackground(theme) {
  const background = theme.overlay && theme.overlay.background;
  if (!background) return null;
  if (!path.isAbsolute(background)) throw new Error('主题背景路径必须是绝对路径');
  const imagePath = background;
  if (!fs.existsSync(imagePath)) throw new Error(`背景图不存在: ${imagePath}`);
  const extension = path.extname(imagePath).toLowerCase();
  const mime = {
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.png': 'image/png',
    '.webp': 'image/webp',
  }[extension];
  if (!mime) throw new Error(`不支持的背景图格式: ${extension}`);
  return `data:${mime};base64,${fs.readFileSync(imagePath).toString('base64')}`;
}

function buildInstallerExpression(theme, css, backgroundDataUri) {
  const overlay = theme.overlay || {};
  const backgroundPosition = typeof overlay.backgroundPosition === 'string'
    ? overlay.backgroundPosition
    : 'center';
  const backgroundSize = typeof overlay.backgroundSize === 'string'
    ? overlay.backgroundSize
    : 'cover';

  return `(() => {
    const install = async () => {
      if (!document.head || !document.body) {
        await new Promise(resolve => document.addEventListener('DOMContentLoaded', resolve, { once: true }));
        return install();
      }

      let style = document.getElementById(${JSON.stringify(STYLE_ID)});
      if (!style) {
        style = document.createElement('style');
        style.id = ${JSON.stringify(STYLE_ID)};
        document.head.appendChild(style);
      }
      style.textContent = ${JSON.stringify(css)};
      document.documentElement.classList.remove('wbskin-proof');
      document.getElementById('wbskin-proof-style')?.remove();
      document.documentElement.classList.add('wbskin-active');

      const dataUri = ${JSON.stringify(backgroundDataUri)};
      document.getElementById(${JSON.stringify(BACKGROUND_ID)})?.remove();
      if (window.__WBSKIN_ART_URL__) URL.revokeObjectURL(window.__WBSKIN_ART_URL__);
      delete window.__WBSKIN_ART_URL__;
      if (dataUri) {
        const separator = dataUri.indexOf(',');
        const metadata = dataUri.slice(0, separator);
        const binary = atob(dataUri.slice(separator + 1));
        const bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
        const mime = /^data:([^;,]+)/.exec(metadata)?.[1] || 'application/octet-stream';
        const blob = new Blob([bytes], { type: mime });
        const artUrl = URL.createObjectURL(blob);
        window.__WBSKIN_ART_URL__ = artUrl;
        document.documentElement.style.setProperty('--wbskin-art', 'url(' + JSON.stringify(artUrl) + ')');
        document.documentElement.style.setProperty('--wbskin-art-position', ${JSON.stringify(backgroundPosition)});
        document.documentElement.style.setProperty('--wbskin-art-size', ${JSON.stringify(backgroundSize)});
        document.documentElement.classList.add('wbskin-has-art');
      } else {
        document.documentElement.style.removeProperty('--wbskin-art');
        document.documentElement.style.removeProperty('--wbskin-art-position');
        document.documentElement.style.removeProperty('--wbskin-art-size');
        document.documentElement.classList.remove('wbskin-has-art');
      }

      window.__WBSKIN_STATE__ = {
        name: ${JSON.stringify(theme.name || theme.id || 'unknown')},
        key: ${JSON.stringify(theme.runtimeKey || null)},
        appliedAt: Date.now()
      };

      return {
        installed: document.documentElement.classList.contains('wbskin-active'),
        stylePresent: Boolean(document.getElementById(${JSON.stringify(STYLE_ID)})),
        backgroundPresent: dataUri
          ? document.documentElement.classList.contains('wbskin-has-art') &&
            Boolean(document.documentElement.style.getPropertyValue('--wbskin-art'))
          : true,
        state: window.__WBSKIN_STATE__
      };
    };
    return install();
  })()`;
}

function buildVerificationExpression(theme, backgroundDataUri) {
  const { palette } = buildPalette(theme);
  const samples = {
    shell: { selector: '.teams-container', expected: palette.panel },
    main: { selector: '.main-content', expected: palette.background },
    sidebar: { selector: '.conversation-list', expected: palette.panel },
    activeTask: { selector: '.conversation-list-tab-button.active', expected: palette.active },
    sceneTabs: { selector: '.wb-scene-tabs', expected: palette.active },
    sceneActive: { selector: '.wb-scene-tabs__pill--active', expected: palette.accent },
    quickAction: { selector: '.quick-actions__item', expected: palette.panelAlt },
    composer: { selector: '[class*="_mainArea_"]', expected: palette.panelAlt },
  };

  return `(() => {
    const normalize = value => {
      const probe = document.createElement('span');
      probe.style.color = value;
      document.body.appendChild(probe);
      const normalized = getComputedStyle(probe).color;
      probe.remove();
      return normalized;
    };
    const definitions = ${JSON.stringify(samples)};
    const components = {};
    for (const [name, definition] of Object.entries(definitions)) {
      const element = document.querySelector(definition.selector);
      components[name] = {
        selector: definition.selector,
        present: Boolean(element),
        actual: element ? getComputedStyle(element).backgroundColor : null,
        expected: normalize(definition.expected)
      };
      components[name].matched = components[name].present &&
        components[name].actual === components[name].expected;
    }

    const required = ['shell', 'main', 'sidebar'];
    if (document.querySelector(definitions.activeTask.selector)) {
      required.push('activeTask');
    }
    if (document.querySelector('.wb-home-page')) {
      required.push('sceneTabs', 'sceneActive', 'quickAction', 'composer');
    }
    const failures = required.filter(name => !components[name]?.matched);
    const shell = document.querySelector('.teams-container') || document.body;
    const backgroundImage = components.main?.present
      ? getComputedStyle(document.querySelector(definitions.main.selector)).backgroundImage
      : 'none';
    return {
      pass: failures.length === 0,
      failures,
      required,
      components,
      installed: document.documentElement.classList.contains('wbskin-active'),
      stylePresent: Boolean(document.getElementById(${JSON.stringify(STYLE_ID)})),
      backgroundPresent: ${backgroundDataUri ? `document.documentElement.classList.contains('wbskin-has-art') && Boolean(document.documentElement.style.getPropertyValue('--wbskin-art')) && backgroundImage !== 'none'` : 'true'},
      backgroundImage,
      accent: getComputedStyle(shell).getPropertyValue('--wb-brand-primary').trim(),
      background: components.main?.actual || null,
      state: window.__WBSKIN_STATE__
    };
  })()`;
}

async function injectTheme(theme, options = {}) {
  const host = options.host || DEFAULT_CDP_HOST;
  const port = Number(options.port);
  const client = new CDPClient(host, port);
  try {
    const { target, probe } = await connectToWorkBuddy(client, options.timeoutMs);
    const css = buildThemeCSS(theme);
    const { palette } = buildPalette(theme);
    const backgroundDataUri = resolveBackground(theme);
    const installerExpression = buildInstallerExpression(theme, css, backgroundDataUri);
    const installed = await client.evaluate(installerExpression);

    if (!installed.installed || !installed.stylePresent || !installed.backgroundPresent) {
      throw new Error(`主题注入失败: ${JSON.stringify(installed)}`);
    }
    const verificationExpression = buildVerificationExpression(theme, backgroundDataUri);
    const verificationDeadline = Date.now() + Number(options.verificationTimeoutMs || 5000);
    let verification;
    do {
      await new Promise(resolve => setTimeout(resolve, 200));
      verification = await client.evaluate(verificationExpression);
      if (verification.pass && verification.stylePresent && verification.backgroundPresent) break;
    } while (Date.now() < verificationDeadline);
    if (!verification.pass || !verification.stylePresent || !verification.backgroundPresent) {
      throw new Error(`主题组件验证失败: ${verification.failures.join(', ') || '注入状态异常'}`);
    }
    if (verification.accent.toLowerCase() !== palette.accent.toLowerCase()) {
      throw new Error(`主题主色未生效: 期望 ${palette.accent}，实际 ${verification.accent || '空'}`);
    }

    return {
      target: { id: target.id, title: target.title, url: target.url },
      probe,
      verification,
    };
  } finally {
    client.disconnect();
  }
}

async function clearTheme(options = {}) {
  const client = new CDPClient(options.host || DEFAULT_CDP_HOST, Number(options.port));
  try {
    await connectToWorkBuddy(client, options.timeoutMs || 5000);
    return await client.clearAll();
  } finally {
    client.disconnect();
  }
}

async function checkTheme(runtimeKey, options = {}) {
  const client = new CDPClient(options.host || DEFAULT_CDP_HOST, Number(options.port));
  try {
    await connectToWorkBuddy(client, options.timeoutMs || 3000);
    const result = await client.evaluate(`(() => {
      const main = document.querySelector('.main-content');
      return {
        active: document.documentElement.classList.contains('wbskin-active'),
        stylePresent: Boolean(document.getElementById(${JSON.stringify(STYLE_ID)})),
        backgroundPresent: document.documentElement.classList.contains('wbskin-has-art') &&
          Boolean(document.documentElement.style.getPropertyValue('--wbskin-art')) &&
          Boolean(main) && getComputedStyle(main).backgroundImage !== 'none',
        key: window.__WBSKIN_STATE__?.key || null
      };
    })()`);
    return Boolean(
      result.active &&
      result.stylePresent &&
      result.key === runtimeKey &&
      (!options.requireBackground || result.backgroundPresent)
    );
  } finally {
    client.disconnect();
  }
}

module.exports = {
  BACKGROUND_ID,
  CDPClient,
  DEFAULT_CDP_HOST,
  STYLE_ID,
  buildInstallerExpression,
  buildVerificationExpression,
  checkTheme,
  clearTheme,
  connectToWorkBuddy,
  injectTheme,
};
