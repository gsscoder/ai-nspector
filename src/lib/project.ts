import type { Language, Project } from "@/types";

export const STACK: Record<Language, string> = {
  python: "Python",
  dotnet: ".NET",
  typescript: "JavaScript / TypeScript",
  other: "Other",
};

// A root project is named after the repo; the suffix tells them apart.
export const projectLabel = (p: Project) => (p.root === "" ? `${p.name} (root)` : p.name);
