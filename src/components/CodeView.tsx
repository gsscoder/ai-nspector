import { useEffect, useState } from "react";
import { readFile } from "@/api";

const LANGS: Record<string, string> = {
  py: "python",
  pyi: "python",
  cs: "csharp",
  razor: "razor",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "tsx",
  js: "javascript",
  jsx: "jsx",
  mjs: "javascript",
  cjs: "javascript",
  vue: "vue",
  svelte: "svelte",
  rs: "rust",
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
        const dark = document.documentElement.classList.contains("dark");
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

  if (!state) return <p className="p-4 text-sm text-muted-foreground">Loading…</p>;
  if ("error" in state) return <p className="p-4 text-sm text-destructive">{state.error}</p>;
  return (
    <div
      className="min-h-0 flex-1 overflow-auto text-[13px] leading-relaxed [&_pre]:w-max [&_pre]:min-w-full [&_pre]:bg-transparent! [&_pre]:p-4 [&_pre]:outline-none"
      dangerouslySetInnerHTML={{ __html: state.html }} // Shiki output: source text is escaped
    />
  );
}
