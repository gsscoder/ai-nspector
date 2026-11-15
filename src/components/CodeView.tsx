import { useEffect, useState } from "react";
import { readFile } from "@/api";

const LANGS: Record<string, string> = {
  py: "python",
  cs: "csharp",
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  jsx: "jsx",
  mjs: "javascript",
  cjs: "javascript",
};

type State = { html: string } | { error: string } | null;

export default function CodeView({ repoPath, path }: { repoPath: string; path: string }) {
  const [state, setState] = useState<State>(null);

  useEffect(() => {
    let stale = false;
    setState(null);
    (async () => {
      try {
        const text = await readFile(repoPath, path);
        const { codeToHtml } = await import("shiki"); // lazy: big bundle
        const lang = LANGS[path.split(".").pop() ?? ""] ?? "text";
        const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
        const html = await codeToHtml(text, { lang, theme: dark ? "github-dark" : "github-light" });
        if (!stale) setState({ html });
      } catch (e) {
        if (!stale) setState({ error: String(e) });
      }
    })();
    return () => {
      stale = true;
    };
  }, [repoPath, path]);

  if (!state) return <p className="text-sm text-muted-foreground">Loading…</p>;
  if ("error" in state) return <p className="text-sm text-destructive">{state.error}</p>;
  return (
    <div
      className="overflow-auto rounded-md border text-sm [&_pre]:p-4"
      dangerouslySetInnerHTML={{ __html: state.html }} // Shiki output: source text is escaped
    />
  );
}
