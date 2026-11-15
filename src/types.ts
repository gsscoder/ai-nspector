// Shared TypeScript types mirroring the Rust structs in src-tauri/src/scan.rs.
export type Language = "python" | "dotnet" | "typescript" | "other";

// Paths are relative to the repo root and use forward slashes.
export interface Project {
  id: string;
  name: string;
  language: Language;
  root: string;
  files: string[];
}

export interface Repo {
  root: string;
  name: string;
  projects: Project[];
}

// Mirror the structs in src-tauri/src/store.rs.
export interface ModuleFile {
  path: string;
  primary: boolean;
}

export interface Module {
  id: string;
  name: string;
  purpose: string;
  files: ModuleFile[];
}

export interface ModulesFile {
  project: string;
  cacheKey: string;
  model: string;
  generatedAt: string;
  modules: Module[];
  outdated: boolean;
}
