import { describe, expect, it, vi } from "vitest";
import type { ThemeLibrary, WorkBuddyStatus } from "../types";
import {
  importThemePackages,
  invokeWithConfirmation,
  loadCoreData,
  RESTART_CONFIRMATION_REQUIRED,
  runMutationAndRefresh,
  type InvokeFunction,
} from "./tauri";

describe("runMutationAndRefresh", () => {
  it("refreshes state after a partially successful mutation fails", async () => {
    const refresh = vi.fn().mockResolvedValue(undefined);

    await expect(runMutationAndRefresh(
      async () => { throw new Error("apply failed"); },
      refresh,
    )).rejects.toThrow("apply failed");

    expect(refresh).toHaveBeenCalledOnce();
  });

  it("preserves the mutation error when refreshing also fails", async () => {
    await expect(runMutationAndRefresh(
      async () => { throw new Error("apply failed"); },
      async () => { throw new Error("refresh failed"); },
    )).rejects.toThrow("apply failed");
  });
});

describe("importThemePackages", () => {
  it("continues after a failed package and reports the partial result", async () => {
    const imported = { manifest: { id: "one" } } as unknown as import("../types").InstalledTheme;
    const importOne = vi.fn(async (path: string) => {
      if (path === "/broken.wbskin") throw new Error("invalid package");
      if (path === "/cancelled.wbskin") return null;
      return imported;
    });

    await expect(importThemePackages(
      ["/one.wbskin", "/broken.wbskin", "/cancelled.wbskin", "/two.wbskin"],
      importOne,
    )).resolves.toEqual({
      imported: 2,
      skipped: 1,
      failures: [{ path: "/broken.wbskin", error: "Error: invalid package" }],
    });
    expect(importOne).toHaveBeenCalledTimes(4);
  });
});

describe("invokeWithConfirmation", () => {
  it("retries only after the user confirms the requested action", async () => {
    const invoke = vi.fn()
      .mockRejectedValueOnce(new Error(RESTART_CONFIRMATION_REQUIRED))
      .mockResolvedValueOnce("done") as unknown as InvokeFunction;
    const confirm = vi.fn(() => true);

    const result = await invokeWithConfirmation(
      invoke,
      confirm,
      "apply_theme",
      { id: "hello-kitty" },
      "restartConfirmed",
      RESTART_CONFIRMATION_REQUIRED,
      "确认重启",
    );

    expect(result).toBe("done");
    expect(confirm).toHaveBeenCalledWith("确认重启");
    expect(invoke).toHaveBeenNthCalledWith(1, "apply_theme", { id: "hello-kitty", restartConfirmed: false });
    expect(invoke).toHaveBeenNthCalledWith(2, "apply_theme", { id: "hello-kitty", restartConfirmed: true });
  });

  it("does not retry when the user cancels", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error(RESTART_CONFIRMATION_REQUIRED)) as unknown as InvokeFunction;

    const result = await invokeWithConfirmation(
      invoke,
      () => false,
      "apply_theme",
      {},
      "restartConfirmed",
      RESTART_CONFIRMATION_REQUIRED,
      "确认重启",
    );

    expect(result).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("preserves unrelated command errors", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error("network failed")) as unknown as InvokeFunction;

    await expect(invokeWithConfirmation(
      invoke,
      () => true,
      "apply_theme",
      {},
      "restartConfirmed",
      RESTART_CONFIRMATION_REQUIRED,
      "确认重启",
    )).rejects.toThrow("network failed");
  });
});

describe("loadCoreData", () => {
  it("loads the product core independently from autostart integration", async () => {
    const status = { installed: true } as WorkBuddyStatus;
    const library: ThemeLibrary = { themes: [], brokenThemes: [] };
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_workbuddy_status") return status;
      if (command === "list_themes") return library;
      if (command === "list_preset_themes") return [];
      throw new Error(`unexpected command: ${command}`);
    }) as unknown as InvokeFunction;

    await expect(loadCoreData(invoke)).resolves.toEqual({ status, library, presetThemes: [] });
    expect(invoke).not.toHaveBeenCalledWith("get_autostart_enabled");
  });
});
