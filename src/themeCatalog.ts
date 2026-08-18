import type { InstalledTheme } from "./types";

export function installedThemeIds(themes: InstalledTheme[]): Set<string> {
  return new Set(themes.map((theme) => theme.manifest.id));
}
