// Shared TypeScript types mirroring the Rust structs in src-tauri/src/scan.rs.
export type Language = "python" | "dotnet" | "typescript" | "other";

export type Role = "backend" | "frontend" | "test" | "unknown";

// Paths are relative to the repo root and use forward slashes.
export interface Project {
  id: string;
  name: string;
  language: Language;
  role: Role;
  root: string;
  files: string[];
}

// Documentation file candidate for the repo overview; size in bytes. The backend sorts suggested files first.
export interface DocFile {
  path: string;
  size: number;
  suggested: boolean;
}

export interface Repo {
  root: string;
  name: string;
  projects: Project[];
  docs: DocFile[];
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

// from imports to: one module imports another; count is the number of file imports.
export interface ModuleRelation {
  from: string;
  to: string;
  count: number;
}

// Mirrors the struct in src-tauri/src/overview.rs. generatedAt is RFC 3339.
export interface OverviewInfo {
  generatedAt: string;
  files: string[];
}

// Mirror the structs in src-tauri/src/analysis.rs. createdAt is RFC 3339.
export interface AnalysisInfo {
  id: string;
  name: string;
  description: string;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface ChatFile {
  analysis: string;
  moduleName: string;
  model: string;
  createdAt: string;
  messages: ChatMessage[];
  outdated: boolean;
}

// Report of one analysis run over several modules of a project. createdAt is RFC 3339.
export interface ModuleReport {
  moduleId: string;
  moduleName: string;
  status: "done" | "nothingFound" | "failed";
  text: string;
  error: string | null;
}

export interface ProjectReport {
  analysis: string;
  model: string;
  createdAt: string;
  cancelled: boolean;
  modules: ModuleReport[];
}

// Payload of the project-analysis-progress event; index is 1-based.
export interface ProjectAnalysisProgress {
  index: number;
  total: number;
  moduleName: string;
}
