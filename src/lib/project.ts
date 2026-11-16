import type { Language, Project } from "@/types";

export const STACK: Record<Language, string> = {
  python: "Python",
  dotnet: ".NET",
  typescript: "JavaScript / TypeScript",
  other: "Other",
};

// A root project is labelled by its role; without a clear role (CLI, monolith) it is just "root".
export const projectLabel = (p: Project) => {
  if (p.root !== "" || p.language === "other") return p.name;
  return p.role === "unknown" ? "root" : `${p.role} (root)`;
};
