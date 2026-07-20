const TOKEN_MAP = {
  '--wb-home-bg-primary': 'panel',
  '--wb-home-bg-secondary': 'background',
  '--wb-sidebar-bg': 'panel',
  '--wb-sidebar-border': 'border',
  '--wb-sidebar-mask-end': 'panel',
  '--wb-bg-primary': 'panelAlt',
  '--wb-bg-secondary': 'subtle',
  '--wb-bg-tertiary': 'active',
  '--wb-bg-hover': 'hover',
  '--wb-bg-hover-light': 'subtle',
  '--wb-bg-active': 'active',
  '--wb-color-bg-primary-default': 'panel',
  '--wb-color-bg-primary-hover': 'hover',
  '--wb-color-bg-primary-active': 'active',
  '--wb-color-bg-secondary-default': 'panelAlt',
  '--wb-color-bg-secondary-hover-active': 'subtle',
  '--wb-palette-gray-2': 'subtle',
  '--wb-palette-gray-3': 'panel',
  '--wb-palette-gray-4': 'hover',
  '--wb-palette-gray-5': 'active',
  '--wb-color-text-primary': 'text',
  '--wb-color-text-secondary': 'muted',
  '--wb-color-text-tertiary': 'muted',
  '--wb-color-text-disabled': 'muted',
  '--wb-color-text-solid': 'text',
  '--wb-text-strong': 'text',
  '--wb-text-medium': 'muted',
  '--wb-text-weak': 'muted',
  '--wb-text-muted': 'muted',
  '--wb-text-white': 'accentText',
  '--wb-text-on-primary': 'accentText',
  '--wb-brand-primary': 'accent',
  '--wb-color-text-brand-default': 'accent',
  '--wb-color-text-brand-hover': 'accent',
  '--wb-color-text-link-default': 'accent',
  '--wb-color-text-link-hover': 'accent',
  '--wb-color-border-primary': 'border',
  '--wb-color-border-secondary': 'border',
  '--wb-color-border-tertiary': 'subtle',
  '--wb-border-default': 'border',
  '--wb-border-weak': 'subtle',
  '--wb-border-subtle': 'subtle',
  '--wb-border-strong': 'border',
  '--wb-todo-menu-bg-hover': 'hover',
  '--wb-todo-menu-bg-active': 'active',
  '--wb-todo-menu-icon-default': 'muted',
  '--wb-todo-menu-text-default': 'text',
  '--wb-todo-menu-text-heading': 'muted',
  '--wb-bg-pill-active': 'accent',
  '--wb-bg-pill-active-hover': 'accent',
  '--wb-bg-pill-hover': 'subtle',
  '--wb-button-primary-bg': 'accent',
  '--wb-button-primary-bg-hover': 'accent',
  '--wb-button-primary-bg-active': 'accent',
  '--wb-button-primary-fg': 'accentText',
  '--wb-button-secondary-bg': 'panelAlt',
  '--wb-button-secondary-bg-hover': 'hover',
  '--wb-button-secondary-bg-active': 'active',
  '--wb-button-secondary-fg': 'text',
  '--wb-button-secondary-border': 'border',
  '--wb-button-grey-bg': 'hover',
  '--wb-button-grey-bg-hover': 'active',
  '--wb-button-grey-fg': 'text',
  '--wb-button-ghost-bg-hover': 'hover',
  '--wb-button-ghost-fg': 'text',
  '--wb-control-selected-bg': 'accent',
  '--wb-control-selected-fg': 'accentText',
  '--wb-quick-action-sub-item-bg': 'panelAlt',
  '--wb-quick-action-item-bg-hover': 'hover',
  '--wb-quick-action-item-border-hover': 'border',
  '--cb-main-area-background': 'panelAlt',
  '--cb-main-area-border-color': 'border',
  '--cb-panel-bg-primary': 'panelAlt',
  '--cb-sidebar-bg': 'panel',
  '--cb-text-primary': 'text',
  '--cb-text-secondary': 'muted',
  '--cb-text-tertiary': 'muted',
  '--cb-green-color': 'accent',
  '--cb-success-color': 'accent',
  '--cb-switch-active-bg': 'accent',
  '--cb-border-secondary': 'border',
  '--cb-hover-bg': 'hover',
  '--cb-icon-button-hover-background': 'hover',
  '--cb-popover-background': 'panelAlt',
  '--cb-popover-bg-color': 'panelAlt',
  '--cb-dropdown-bg-color': 'panelAlt',
};

function firstColor(colors, keys, fallback) {
  for (const key of keys) {
    const value = colors[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return fallback;
}

function buildPalette(theme) {
  const colors = theme.vscode && theme.vscode.colors ? theme.vscode.colors : {};
  const workbuddy = theme.workbuddy || {};
  const palette = {
    background: firstColor(colors, ['editor.background', 'panel.background'], '#fff8fb'),
    panel: firstColor(colors, ['sideBar.background', 'editorWidget.background'], '#fdebf1'),
    panelAlt: firstColor(colors, ['input.background', 'dropdown.background'], '#ffffff'),
    text: firstColor(colors, ['editor.foreground', 'sideBarTitle.foreground'], '#4a2934'),
    muted: firstColor(
      colors,
      ['sideBar.foreground', 'input.placeholderForeground', 'terminal.foreground'],
      '#75505d'
    ),
    accent: firstColor(colors, ['button.background', 'activityBarBadge.background'], '#d95f8d'),
    accentText: firstColor(colors, ['button.foreground', 'activityBarBadge.foreground'], '#ffffff'),
    border: firstColor(colors, ['panel.border', 'input.border', 'editorWidget.border'], '#edb8cb'),
    hover: firstColor(colors, ['list.hoverBackground', 'button.hoverBackground'], '#f8dbe6'),
    active: firstColor(colors, ['list.activeSelectionBackground'], '#f3c7d7'),
    subtle: firstColor(colors, ['list.inactiveSelectionBackground'], '#fff2f7'),
    ...(workbuddy.palette || {}),
  };

  const tokens = Object.fromEntries(
    Object.entries(TOKEN_MAP).map(([token, paletteKey]) => [token, palette[paletteKey]])
  );

  return {
    palette,
    tokens: { ...tokens, ...(workbuddy.tokens || {}) },
  };
}

function inferColorScheme(color) {
  if (typeof color !== 'string' || !/^#[0-9a-fA-F]{6,8}$/.test(color)) return 'light';
  const channels = [1, 3, 5].map(index => {
    const value = parseInt(color.slice(index, index + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  const luminance = channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  return luminance < 0.35 ? 'dark' : 'light';
}

function buildThemeCSS(theme) {
  const { palette, tokens } = buildPalette(theme);
  const colorScheme = inferColorScheme(palette.background);
  const declarations = Object.entries(tokens)
    .map(([token, value]) => `  ${token}: ${value} !important;`)
    .join('\n');

  return `
html.wbskin-active,
html.wbskin-active body,
html.wbskin-active .light,
html.wbskin-active .cb-light,
html.wbskin-active .vscode-light,
html.wbskin-active .agent-ui-theme,
html.wbskin-active .teams-container,
html.wbskin-active .wb-cb-chat.light {
${declarations}
  color-scheme: ${colorScheme} !important;
}

html.wbskin-active,
html.wbskin-active body {
  background: ${palette.background} !important;
  color: ${palette.text} !important;
}

html.wbskin-active .teams-container {
  background-color: ${palette.background} !important;
  color: ${palette.text} !important;
}

html.wbskin-active.wbskin-has-art .teams-container {
  background-image: var(--wbskin-art) !important;
  background-position: var(--wbskin-art-position, center) !important;
  background-repeat: no-repeat !important;
  background-size: var(--wbskin-art-size, contain) !important;
}

html.wbskin-active .teams-container [class*="_gridViewItem_"],
html.wbskin-active .teams-content-wrapper,
html.wbskin-active .teams-main-content,
html.wbskin-active .main-content {
  background: transparent !important;
  color: ${palette.text} !important;
}

html.wbskin-active .workbuddy-topbar {
  background: color-mix(in srgb, ${palette.panel} 36%, transparent) !important;
  border-bottom: 1px solid color-mix(in srgb, ${palette.border} 55%, transparent) !important;
  backdrop-filter: blur(14px) saturate(115%);
  -webkit-backdrop-filter: blur(14px) saturate(115%);
}

html.wbskin-active .conversation-sidebar {
  background: color-mix(in srgb, ${palette.panel} 68%, transparent) !important;
  border-right: 1px solid color-mix(in srgb, ${palette.border} 60%, transparent) !important;
  color: ${palette.text} !important;
  backdrop-filter: blur(18px) saturate(115%);
  -webkit-backdrop-filter: blur(18px) saturate(115%);
}

html.wbskin-active .conversation-list,
html.wbskin-active .collapsible-section-header {
  background: transparent !important;
  color: ${palette.text} !important;
}

html.wbskin-active .conversation-list-tab-button-box.active {
  background: color-mix(in srgb, ${palette.active} 78%, transparent) !important;
  color: ${palette.text} !important;
  box-shadow: inset 3px 0 ${palette.accent} !important;
}

html.wbskin-active .conversation-list-tab-button:hover,
html.wbskin-active [role="button"]:hover {
  background: color-mix(in srgb, ${palette.hover} 68%, transparent) !important;
}

html.wbskin-active .wb-scene-tabs {
  background: color-mix(in srgb, ${palette.panelAlt} 64%, transparent) !important;
  border: 1px solid color-mix(in srgb, ${palette.border} 70%, transparent) !important;
  backdrop-filter: blur(12px) saturate(115%);
  -webkit-backdrop-filter: blur(12px) saturate(115%);
}

html.wbskin-active .wb-scene-tabs__pill--active {
  background: ${palette.accent} !important;
  color: ${palette.accentText} !important;
}

html.wbskin-active .quick-actions__item {
  background: color-mix(in srgb, ${palette.panelAlt} 74%, transparent) !important;
  border: 1px solid color-mix(in srgb, ${palette.border} 72%, transparent) !important;
  color: ${palette.text} !important;
  box-shadow: 0 5px 14px color-mix(in srgb, ${palette.accent} 12%, transparent) !important;
  backdrop-filter: blur(12px) saturate(115%);
  -webkit-backdrop-filter: blur(12px) saturate(115%);
}

html.wbskin-active .quick-actions__item:hover {
  background: color-mix(in srgb, ${palette.hover} 82%, transparent) !important;
  border-color: ${palette.accent} !important;
  transform: translateY(-1px);
}

html.wbskin-active [class*="_mainArea_"],
html.wbskin-active .wb-home-composer__input-slot [class*="mainArea"] {
  background: color-mix(in srgb, ${palette.panelAlt} 82%, transparent) !important;
  border: 1px solid color-mix(in srgb, ${palette.border} 78%, transparent) !important;
  box-shadow: 0 16px 38px color-mix(in srgb, ${palette.accent} 16%, transparent) !important;
  backdrop-filter: blur(20px) saturate(115%);
  -webkit-backdrop-filter: blur(20px) saturate(115%);
}

html.wbskin-active .cb-button--primary,
html.wbskin-active .wb-button--primary {
  background: ${palette.accent} !important;
  color: ${palette.accentText} !important;
}

`;
}

module.exports = {
  buildPalette,
  buildThemeCSS,
  inferColorScheme,
};
