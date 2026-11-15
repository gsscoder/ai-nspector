# ai-nspector spec
- Audience: AI coding agents; read with `intent.md` (purpose and concepts)
- Status: decided with the user on 2026-11-14; no code yet
- Items marked "default" were not asked; change them only with a reason
## App shell
- Tauri 2: Rust backend, React frontend, one executable
- No sidecar processes: all code analysis runs in Rust inside the app process
- Frontend: React + TypeScript + Vite (Tauri template)
- Styling: Tailwind + shadcn/ui
- Code view: Shiki, read-only
- File sidebar: flat list; file name, relative folder path in grey text
- Platforms: Windows and Linux; no macOS for now
## Target repos (MVP)
- Python, .NET (C#), TypeScript/JavaScript; all 3 in the MVP
- Parser: tree-sitter Rust crates, one grammar per language
- Project detection: manifest files, no AI (list in `intent.md`)
  - .NET: each `.csproj` is a project; `.sln` files are ignored
  - Nesting: a file belongs to the nearest manifest above it
  - Workspace root (e.g. `package.json` with `workspaces`) with no own source files is not a project
  - Source files under no manifest: one extra project named "(other files)"
- Import graph per language:
  - Python: `import` / `from ... import` → resolve module path to files
  - TS/JS: relative `import` / `require` → files; bare package names are external, skip them
  - C#: `using` gives namespaces → map to files through the `namespace` declarations in each file
- File walk: respect `.gitignore` (`ignore` crate) and skip `bin/`, `obj/`, `node_modules/`, `.venv/`, `dist/` (default)
## LLM
- One client: OpenAI-compatible Chat Completions API
- Settings: base URL, model name, API key
- Works with OpenAI, OpenRouter (Claude), Azure OpenAI, Ollama, LM Studio; local models solve the privacy risk
- API key: OS credential store via `keyring` crate (Windows Credential Manager, Linux Secret Service)
- Module generation: AI only, input is structure (file tree + imports + top-level symbols); no deterministic clustering in the MVP
- Output: JSON; validate with serde; invalid → retry once, then show an error (default)
- Input too large for the context: drop symbols first; still too large → error with the file count; batch split is later
## Modules
- Each file has exactly 1 primary module and can also appear in other modules
- Invariant: every project file has a primary module
- Cache file shape (sketch):
```json
{
  "project": "Api.Core",
  "cacheKey": "git:3f2a9c1",
  "model": "model-name",
  "generatedAt": "2026-11-14T10:00:00Z",
  "modules": [
    {
      "id": "billing",
      "name": "Billing",
      "purpose": "Invoices and tax calculation",
      "files": [{ "path": "src/Billing/InvoiceService.cs", "primary": true }]
    }
  ]
}
```
## Storage and cache
- Repo-specific data lives in the inspected repo: `<repo>/.ainspector/` (modules now, analysis findings later)
  - Git-ignored by default: on first create, the app writes `.ainspector/.gitignore` with the content `*`; the repo's own `.gitignore` is never edited
  - Team sharing: delete `.ainspector/.gitignore` and commit the folder
  - All paths inside are relative to the repo root (portable across clones and moves)
  - The file walk skips `.ainspector/`
  - Never write secrets here (API key, base URL)
  - No write permission: show an error (default); app-data fallback only if needed later
- Global data stays in the app data folder: settings, recent repos list; API key in the OS credential store
- Format: JSON, one file per project, overwritten on Regenerate; `cacheKey` inside detects "outdated"; no database
- Git is optional
  - git repo: key = commit hash; uncommitted changes to project files also mark the cache outdated
  - no git: key = hash of file paths + sizes + modified times
- Repo changed: show the old modules with an "outdated" badge; the user clicks Regenerate; no automatic LLM calls
## Repo handling
- One repo open at a time
- Start screen: list of recent repos, stored in the app data folder
## Later (not MVP)
- macOS build
- Batch split for large projects
- Deterministic clustering before the LLM
- SQLite, if the future analysis findings need queries
