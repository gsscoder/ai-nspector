import type { Language, Project } from "@/types";

export const STACK: Record<Language, string> = {
  python: "Python",
  dotnet: ".NET",
  typescript: "JavaScript / TypeScript",
  rust: "Rust",
  other: "Other",
};

export const stackLabel = (p: Project) => p.languages.map((language) => STACK[language]).join(" + ");

export const splitPath = (path: string) => {
  const i = path.lastIndexOf("/");
  return { name: path.slice(i + 1), dir: i < 0 ? "" : path.slice(0, i) };
};

// A root project is labelled by its role; without a clear role (monolith) it is just "root".
export const projectLabel = (p: Project) => {
  if (p.root !== "" || p.language === "other") return p.name;
  return p.role === "unknown" ? "root" : `${p.role} (root)`;
};
