import { invoke } from "@tauri-apps/api/core";
import type { InstalledTheme, ThemeLibrary, WorkBuddyStatus } from "../types";

export const RESTART_CONFIRMATION_REQUIRED = "WORKBUDDY_RESTART_CONFIRMATION_REQUIRED";
export const THEME_OVERWRITE_CONFIRMATION_REQUIRED = "THEME_OVERWRITE_CONFIRMATION_REQUIRED";

export type InvokeFunction = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface BatchImportResult {
  imported: number;
  skipped: number;
  failures: Array<{ path: string; error: string }>;
}

export async function importThemePackages(
  paths: string[],
  importOne: (path: string) => Promise<InstalledTheme | null>,
): Promise<BatchImportResult> {
  const result: BatchImportResult = { imported: 0, skipped: 0, failures: [] };
  for (const path of paths) {
    try {
      if (await importOne(path)) {
        result.imported += 1;
      } else {
        result.skipped += 1;
      }
    } catch (error) {
      result.failures.push({ path, error: String(error) });
    }
  }
  return result;
}

export async function runMutationAndRefresh<T>(
  mutation: () => Promise<T>,
  refresh: () => Promise<void>,
): Promise<T> {
  let result: T | undefined;
  let mutationError: unknown;
  let mutationFailed = false;
  try {
    result = await mutation();
  } catch (error) {
    mutationFailed = true;
    mutationError = error;
  }
  try {
    await refresh();
  } catch (error) {
    if (!mutationFailed) throw error;
  }
  if (mutationFailed) throw mutationError;
  return result as T;
}

export async function invokeWithConfirmation<T>(
  invokeFunction: InvokeFunction,
  confirmFunction: (message: string) => boolean,
  command: string,
  args: Record<string, unknown>,
  confirmationArgument: string,
  requiredMarker: string,
  message: string,
): Promise<T | null> {
  try {
    return await invokeFunction<T>(command, { ...args, [confirmationArgument]: false });
  } catch (error) {
    if (!String(error).includes(requiredMarker)) throw error;
    if (!confirmFunction(message)) return null;
    return invokeFunction<T>(command, { ...args, [confirmationArgument]: true });
  }
}

export function invokeWithRestartConfirmation<T>(
  command: string,
  args: Record<string, unknown>,
  message: string,
): Promise<T | null> {
  return invokeWithConfirmation(
    invoke,
    window.confirm,
    command,
    args,
    "restartConfirmed",
    RESTART_CONFIRMATION_REQUIRED,
    message,
  );
}

export function invokeWithOverwriteConfirmation<T>(
  command: string,
  args: Record<string, unknown>,
  message: string,
): Promise<T | null> {
  return invokeWithConfirmation(
    invoke,
    window.confirm,
    command,
    args,
    "overwriteConfirmed",
    THEME_OVERWRITE_CONFIRMATION_REQUIRED,
    message,
  );
}

export async function loadCoreData(invokeFunction: InvokeFunction = invoke) {
  const [status, library, presetThemes] = await Promise.all([
    invokeFunction<WorkBuddyStatus>("get_workbuddy_status"),
    invokeFunction<ThemeLibrary>("list_themes"),
    invokeFunction<InstalledTheme[]>("list_preset_themes"),
  ]);
  return { status, library, presetThemes };
}

export function loadAutostart(invokeFunction: InvokeFunction = invoke) {
  return invokeFunction<boolean>("get_autostart_enabled");
}
