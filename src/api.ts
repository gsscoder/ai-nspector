import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ModulesFile, Repo } from "@/types";

export const openRepo = (path: string) => invoke<Repo>("open_repo", { path });

export const listRecentRepos = () => invoke<string[]>("list_recent_repos");

// Resolves to null when the user cancels the dialog.
export const pickFolder = () => open({ directory: true });

export const loadModules = (repoPath: string, projectId: string) =>
  invoke<ModulesFile | null>("load_modules", { repoPath, projectId });

export const generateModules = (repoPath: string, projectId: string) =>
  invoke<ModulesFile>("generate_modules", { repoPath, projectId });

export const cancelGeneration = () => invoke<void>("cancel_generation");

// Returns the unlisten function.
export const onModulesProgress = (cb: (stage: string) => void) =>
  listen<{ stage: string }>("modules-progress", (e) => cb(e.payload.stage));

export const readFile = (repoPath: string, path: string) =>
  invoke<string>("read_file", { repoPath, path });

// Opens in VS Code (the only app the capability allows: a default handler could run a .py/.js file).
// Falls back to the file manager when VS Code is not found.
export async function openInEditor(repoRoot: string, path: string) {
  const full = `${repoRoot}/${path}`;
  try {
    await openPath(full, "code");
  } catch {
    await revealItemInDir(full);
  }
}
