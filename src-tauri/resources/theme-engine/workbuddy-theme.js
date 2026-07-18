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
    muted: firstColor(colors, ['sideBarTitle.foreground', 'terminal.foreground'], '#75505d'),
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

function buildThemeCSS(theme) {
  const { palette, tokens } = buildPalette(theme);
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
  color-scheme: light !important;
}

html.wbskin-active,
html.wbskin-active body,
html.wbskin-active .teams-container {
  background: ${palette.panel} !important;
  color: ${palette.text} !important;
}

html.wbskin-active .teams-main-content,
html.wbskin-active .main-content {
  background-color: ${palette.background} !important;
}

html.wbskin-active.wbskin-has-art .main-content {
  background-image: var(--wbskin-art) !important;
  background-position: var(--wbskin-art-position, center) !important;
  background-repeat: no-repeat !important;
  background-size: var(--wbskin-art-size, cover) !important;
}

html.wbskin-active .conversation-sidebar,
html.wbskin-active .conversation-list,
html.wbskin-active .collapsible-section-header {
  background: ${palette.panel} !important;
}

html.wbskin-active .conversation-list-tab-button-box.active {
  background: ${palette.active} !important;
  color: ${palette.text} !important;
  box-shadow: inset 3px 0 ${palette.accent} !important;
}

html.wbskin-active .conversation-list-tab-button:hover,
html.wbskin-active [role="button"]:hover {
  background: ${palette.hover} !important;
}

html.wbskin-active .wb-scene-tabs {
  background: ${palette.active} !important;
  border: 1px solid ${palette.border} !important;
}

html.wbskin-active .wb-scene-tabs__pill--active {
  background: ${palette.accent} !important;
  color: ${palette.accentText} !important;
}

html.wbskin-active .quick-actions__item {
  background: ${palette.panelAlt} !important;
  border: 1px solid ${palette.border} !important;
  color: ${palette.text} !important;
  box-shadow: 0 5px 14px color-mix(in srgb, ${palette.accent} 12%, transparent) !important;
}

html.wbskin-active .quick-actions__item:hover {
  background: ${palette.hover} !important;
  border-color: ${palette.accent} !important;
  transform: translateY(-1px);
}

html.wbskin-active [class*="_mainArea_"],
html.wbskin-active .wb-home-composer__input-slot [class*="mainArea"] {
  background: ${palette.panelAlt} !important;
  border: 1px solid ${palette.border} !important;
  box-shadow: 0 16px 38px color-mix(in srgb, ${palette.accent} 16%, transparent) !important;
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
};
