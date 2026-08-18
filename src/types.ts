export interface Compatibility {
  manager: string;
  workbuddy: string[];
}

export interface ThemeManifest {
  id: string;
  name: string;
  author: string;
  description: string;
  compatibility: Compatibility;
}

export interface InstalledTheme {
  manifest: ThemeManifest;
  theme: { palette: { background: string; accent: string } };
  previewPath: string | null;
  backgroundPath: string;
}

export interface BrokenTheme {
  id: string;
  reason: string;
}

export interface ThemeLibrary {
  themes: InstalledTheme[];
  brokenThemes: BrokenTheme[];
}

export interface ThemeLibraryBackup {
  count: number;
  path: string;
}

export interface WorkBuddyStatus {
  installed: boolean;
  running: boolean;
  appPath: string;
  version: string | null;
  managerCompatible: boolean;
  cdpAvailable: boolean;
  cdpPort: number | null;
  activeThemeId: string | null;
  configuredThemeId: string | null;
  restartRequired: boolean;
  customPath: boolean;
}

export interface DiagnosticInfo {
  managerVersion: string;
  platform: string;
  logPath: string;
  recentErrors: string[];
  workbuddy: WorkBuddyStatus | null;
  statusError: string | null;
}

export interface Notice {
  tone: "success" | "error" | "info";
  message: string;
}
