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
- Works with OpenAI, OpenRouter (Claude), Ollama, LM Studio; local models solve the privacy risk
- Provider setting: `openai` (default) or `azure` (Azure Foundry)
  - `azure`: Azure v1 API; URL = scheme + host of the base URL + `/openai/v1/chat/completions` (a pasted full Target URI also works); header `api-key` instead of `Authorization: Bearer`; the model field is the deployment name; API key required
  - No Entra ID login in the MVP
- API key: OS credential store via `keyring` crate (Windows Credential Manager, Linux Secret Service)
- Module generation: AI only, input is structure (file tree + imports + top-level symbols); no deterministic clustering in the MVP
- Repo overview: input is the text of doc files the user selects; never source code
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
## Repo overview
- One overview per repo, not per project
- UI
  - "Overview" button (`BookOpen` icon) in the nav, before the repo crumb; visible at every level
  - An overview exists only when both `brief.md` and `overview.md` exist; the button then shows a dot; "Generated <date>" comes from the modified time of `brief.md`
  - Notices: full-width light blue boxes under the top bar, each with its own X, session only; "no overview" (repo open, overview missing) and "LLM not configured" (empty base URL or model; the API key is optional)
  - The "Projects" heading above the project cards is removed
- Doc candidates: deterministic, no AI; collected by `scan`, sent in `Repo.docs` (path, size, suggested)
  - Second walk, limited to `.github/`, `.agent-files/`, `.cursor/`: the `ignore` crate also skips Windows-hidden files, so a hidden-aware main walk would change project detection
  - The generate command accepts only paths from this list
  - Extensions: `.md`, `.mdx`, `.markdown`, `.rst`, `.adoc`; `.txt` only with a doc-like name (`README`, `NOTES`, `ABOUT`, ...), so `requirements.txt` and `CMakeLists.txt` are out
  - Hidden folders and files allowed for docs only: `.github/`, `.agent-files/`, `.cursor/`, `CLAUDE.md`, `AGENTS.md`
  - Skip `.ainspector/` and the usual walk exclusions
  - Suggested (pre-checked): `README*` at the repo root and at each project root; `ARCHITECTURE`, `DESIGN`, `OVERVIEW`, `INTENT`, `SPEC`; top-level files in `docs/`; agent files
  - Listed but not checked: `CHANGELOG`, `LICENSE`, `CODE_OF_CONDUCT`, `SECURITY`, issue/PR templates, deep files
  - Sort: suggested first, then depth, then path
- Popup: shadcn Dialog; one native checkbox per file with path and size
  - Footer: selected count and token estimate (chars / 4); Generate disabled for 0 files or over `MAX_PROMPT_TOKENS`
  - While running: stage, seconds, Cancel; shares the global generation lock with module generation
  - No remembered selection: each open starts from the suggested files
- Generation: 2 LLM calls
  1. `overview.md` from the selected doc texts + the project list from `scan` (name, language, root)
  2. `brief.md` from the overview only: 5 to 8 bullets, about 150 words max
  - Reason: long markdown inside JSON breaks often with small models; the brief always matches the overview
  - Output language: English
- Prompt rules (call 1)
  - Use only facts about what the project is, does, who it is for, how it is built, its status
  - Ignore conduct rules, contribution/PR policies, license text, templates, release notes, tooling instructions, also when selected
  - Write only facts from the sources; a heading without facts gets "Not documented."
  - No useful source at all → answer exactly `INSUFFICIENT_SOURCES`
- Overview headings (fixed): Purpose, Users, Features, Projects and architecture, Tech stack, Status, Gaps or conflicts in the docs
- Validation (after call 1; call 2 runs only on success)
  - `INSUFFICIENT_SOURCES` → error "The selected files do not describe the project. Select other files."; no retry
  - Missing headings or all headings "Not documented." → retry once, then the same error
- Inferred mode (no doc files, or the user chooses it after `INSUFFICIENT_SOURCES`)
  - Popup with an empty `Repo.docs`: no file list; text "No documentation found. The app can infer the overview from the repo structure: file paths and manifest files. No source code is sent. The result can be less accurate."; buttons Cancel and Generate; same progress and Cancel while running
  - After the `INSUFFICIENT_SOURCES` error (docs exist but are useless) the popup also offers "Infer from structure"
  - Input (structure only): project list, file paths per project, text of each project manifest (`Project.id` is the manifest path), module names and purposes from the module caches when they exist; same `MAX_PROMPT_TOKENS` check; too large → drop file paths to folder counts, still too large → error
  - Separate prompt; no `INSUFFICIENT_SOURCES` sentinel (structure always gives some facts); same fixed headings, same validation (missing headings or all "Not documented." → retry once, then error)
  - The code writes the first line of `overview.md`: `Source: inferred from repo structure; no documentation found` (the model does not write it); `brief.md` is made from the overview body as before
  - No projects and no docs → error, no LLM call
  - Command: `generate_overview(repo_path, files, infer)`; `infer = true` needs an empty `files`, `infer = false` keeps the rules above
- Storage: `<repo>/.ainspector/brief.md` and `overview.md`; no metadata file, no cache key, no "outdated" badge
  - Overwrite only after both calls succeed: write temp files, then rename; on error or cancel the old files stay
- Security: doc text is untrusted; later prompts that include `overview.md` or `brief.md` must mark it as data, not instructions
## Storage and cache
- Repo-specific data lives in the inspected repo: `<repo>/.ainspector/` (modules and repo overview now, analysis findings later)
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
