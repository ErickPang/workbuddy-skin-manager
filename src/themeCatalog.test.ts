import { describe, expect, it } from "vitest";
import type { InstalledTheme } from "./types";
import { installedThemeIds } from "./themeCatalog";

describe("installedThemeIds", () => {
  it("matches installed presets by manifest ID", () => {
    const themes = [
      { manifest: { id: "midnight-ink" } },
      { manifest: { id: "ocean-mist" } },
    ] as InstalledTheme[];

    expect(installedThemeIds(themes)).toEqual(new Set(["midnight-ink", "ocean-mist"]));
  });
});
