import { memo, useEffect, useState } from "react";
import ReactMarkdown, { type Components, type ExtraProps } from "react-markdown";
import remarkBreaks from "remark-breaks";
import { cn } from "@/lib/utils";

type Highlight = { text: string; lang: string; html: string };

// a streamed block changes on every delta: highlight only once it stops changing
const HIGHLIGHT_DELAY_MS = 300;

// fenced code arrives as <pre><code class="language-x">text</code></pre>; v10 has no `inline` prop
function fencedCode(pre: ExtraProps["node"]): { text: string; lang: string } {
  const code = pre?.children[0];
  if (code?.type !== "element") return { text: "", lang: "text" };
  const text = code.children.map((child) => (child.type === "text" ? child.value : "")).join("");
  const classes = code.properties.className;
  const langClass = Array.isArray(classes)
    ? classes.find((c): c is string => typeof c === "string" && c.startsWith("language-"))
    : undefined;
  return { text: text.replace(/\n$/, ""), lang: langClass?.slice("language-".length) ?? "text" };
}

function CodeBlock({ text, lang }: { text: string; lang: string }) {
  const [highlight, setHighlight] = useState<Highlight | null>(null);

  useEffect(() => {
    let stale = false;
    const timer = setTimeout(async () => {
      const { bundledLanguages, codeToHtml } = await import("shiki"); // lazy: big bundle
      const dark = document.documentElement.classList.contains("dark");
      const html = await codeToHtml(text, {
        lang: Object.keys(bundledLanguages).includes(lang) ? lang : "text", // Shiki throws on an unknown language
        theme: dark ? "github-dark" : "github-light",
      });
      if (!stale) setHighlight({ text, lang, html });
    }, HIGHLIGHT_DELAY_MS);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [text, lang]);

  // the highlight of an older text (still streaming) never shows: plain block until it catches up
  if (highlight?.text !== text || highlight.lang !== lang) return <pre>{text}</pre>;
  return <div dangerouslySetInnerHTML={{ __html: highlight.html }} />; // Shiki output: source text is escaped
}

const components: Components = {
  pre: ({ node }) => <CodeBlock {...fencedCode(node)} />,
  // a navigation would move the webview away from the app
  a: ({ href, children }) => (
    <span title={href} className="underline decoration-dotted underline-offset-2">
      {children}
    </span>
  ),
};

function Markdown({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn("markdown", className)}>
      {/* untrusted text: raw HTML stays off (default); images off, so no remote request */}
      <ReactMarkdown components={components} disallowedElements={["img"]} remarkPlugins={[remarkBreaks]}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

export default memo(Markdown);
