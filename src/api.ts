import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AnalysisInfo, ChatFile, ModulesFile, OverviewInfo, Repo } from "@/types";

export const openRepo = (path: string) => invoke<Repo>("open_repo", { path });

export const listRecentRepos = () => invoke<string[]>("list_recent_repos");

// Resolves to null when the user cancels the dialog.
export const pickFolder = () => open({ directory: true });

export const loadModules = (repoPath: string, projectId: string) =>
  invoke<ModulesFile | null>("load_modules", { repoPath, projectId });

export const generateModules = (repoPath: string, projectId: string) =>
  invoke<ModulesFile>("generate_modules", { repoPath, projectId });

export const loadOverview = (repoPath: string) => invoke<OverviewInfo | null>("load_overview", { repoPath });

export const generateOverview = (repoPath: string, files: string[], infer: boolean) =>
  invoke<OverviewInfo>("generate_overview", { repoPath, files, infer });

export const cancelGeneration = () => invoke<void>("cancel_generation");

// Returns the unlisten function.
export const onModulesProgress = (cb: (stage: string) => void) =>
  listen<{ stage: string }>("modules-progress", (e) => cb(e.payload.stage));

export const listAnalyses = () => invoke<AnalysisInfo[]>("list_analyses");

export const loadChat = (repoPath: string, projectId: string, moduleId: string) =>
  invoke<ChatFile | null>("load_chat", { repoPath, projectId, moduleId });

export const startAnalysis = (repoPath: string, projectId: string, moduleId: string, analysisId: string) =>
  invoke<ChatFile>("start_analysis", { repoPath, projectId, moduleId, analysisId });

export const sendChatMessage = (repoPath: string, projectId: string, moduleId: string, text: string) =>
  invoke<ChatFile>("send_chat_message", { repoPath, projectId, moduleId, text });

export const discardChat = (repoPath: string, projectId: string, moduleId: string) =>
  invoke<void>("discard_chat", { repoPath, projectId, moduleId });

export const createAgentPrompt = (repoPath: string, projectId: string, moduleId: string) =>
  invoke<string>("create_agent_prompt", { repoPath, projectId, moduleId });

// Returns the unlisten function.
export const onChatDelta = (cb: (text: string) => void) =>
  listen<{ text: string }>("chat-delta", (e) => cb(e.payload.text));

export const readFile = (repoPath: string, path: string) =>
  invoke<string>("read_file", { repoPath, path });

// Opens in VS Code (the only app the capability allows: a default handler could run a .py/.js file).
// Falls back to the file manager when VS Code is not found.
export async function openInEditor(repoRoot: string, path: string) {
  const separator = repoRoot.includes("\\") ? "\\" : "/";
  const full = repoRoot + separator + path.split("/").join(separator);
  try {
    await openPath(full, "code");
  } catch {
    await revealItemInDir(full);
  }
}
