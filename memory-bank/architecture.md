# ai-nspector architecture
- Audience: AI coding agents; machine consumption, not a human audience
- Use: fast mental model before work in the repo; concepts in `../.agent-files/intent.md`, tech decisions in `../.agent-files/spec.md`
## Purpose
- Desktop app: help a user understand a repo they did not write, often AI-generated
- Navigation: repo → project → module → file; navigation is the frame, AI analysis is the product
- Implemented: project detection, AI module split, repo overview; over-engineering and AI-slop judgement come later
## Process model
- Tauri 2, one executable, no sidecar; Windows and Linux
- Rust backend runs all analysis in the app process: walk, parse, LLM calls, cache
- React frontend calls Rust commands through `invoke`; commands are registered in `src-tauri/src/lib.rs`
- Module and overview generation emit progress stages as the `modules-progress` event
## Backend (`src-tauri/src/`)
- `scan.rs` — repo walk (`ignore` crate, respects `.gitignore`, skips build/vendor dirs and `.ainspector/`), project detection from manifests, doc candidates, recent repos; `open_repo`, `list_recent_repos`
- `parse.rs` + `parse/` (`python.rs`, `csharp.rs`, `script.rs` for TS/JS, `paths.rs`) — tree-sitter imports and top-level symbols per file; syntactic, never runs a language tool chain
- `modules.rs` — module generation: structure prompt, LLM call, validation, cache write; owns the run lock and cancel flag; `load_modules`, `generate_modules`, `cancel_generation`
- `overview.rs` — repo overview: documented or inferred prompt, 2 LLM calls, validation, atomic write; `load_overview`, `generate_overview`
- `llm.rs` — OpenAI-compatible Chat Completions client (`reqwest`); `test_connection`
- `settings.rs` — base URL and model in the app data folder, API key in the OS credential store (`keyring`); `get_settings`, `save_settings`
- `store.rs` — `<repo>/.ainspector/` paths, module cache read/write, cache key
- `files.rs` — `read_file`: read-only, path must resolve inside the open repo, size cap
- `error.rs` — shared `AppError`
## Frontend (`src/`)
- `components/` — screens: `StartScreen`, `RepoScreen`, `RepoView`, `ProjectView`, `ModuleView`, `CodeView` (Shiki), `Settings`, `OverviewDialog`, `Notice`; `components/ui/` is shadcn (Base UI)
- `api.ts` — typed `invoke` wrappers, progress listener, open file in editor or reveal in folder (`@tauri-apps/plugin-opener`)
- `settingsApi.ts` — settings wrappers; API key is write-only, the backend returns only `hasApiKey`
- `types.ts` — mirrors the Rust structs (`Repo`, `Project`, `DocFile`, `ModulesFile`, `OverviewInfo`)
## Flows
```
open_repo         → scan (manifests, no AI) → Repo { projects, docs }
generate_modules  → scan → parse → prompt → LLM → validate (retry once) → .ainspector/modules/
generate_overview → doc texts | structure → LLM: overview.md → validate → LLM: brief.md → .ainspector/
```
### Open repo
- Deterministic: a file belongs to the nearest manifest above it; files under no manifest form the `(other files)` project
- Each project gets a `Role` (`backend`, `frontend`, `unknown`) from manifest content only
- Second walk collects doc candidates (`Repo.docs`), some marked suggested; also enters `.github/`, `.agent-files/`, `.cursor/`
- Adds the repo to recent repos
### Module generation
- Input: structure only, one line per file: path, project-internal imports, top-level symbols; never raw source
- Over `MAX_PROMPT_TOKENS`: drop symbols; still too large → error
- Output: JSON; unparsable or no modules → retry once, then error
- Validation repairs the answer: unknown paths dropped, exactly one primary module per file (first claim wins), files left out go to an `Unassigned` module
- `load_modules` reads the cache and computes `outdated`; no automatic LLM call, the user regenerates
### Repo overview
- One per repo, not per project
- Documented mode: text of the user-selected doc files; only paths from `Repo.docs` are accepted
- Inferred mode (`infer = true`, empty `files`): project list, file paths or folder counts, manifest text, module names and purposes from existing caches; never source
- Call 1 writes `overview.md` with fixed headings; missing headings or no facts → retry once; documented mode only: `INSUFFICIENT_SOURCES` sentinel → error, no retry
- Call 2 writes `brief.md` from the overview only
- Inferred result: the code, not the model, writes a source line at the top of `overview.md`
- Write: temp files, then rename; on error or cancel the old files stay
- Exists only when both files exist; date is the modified time of `brief.md`; no cache key, no outdated state
## Storage
- `<repo>/.ainspector/` — repo data: `modules/<project-slug>-<hash>.json` (one per project), `overview.md`, `brief.md`
  - On first create writes `.ainspector/.gitignore` with `*`; the repo's own `.gitignore` is never edited
  - Paths inside are relative to the repo root; no secrets
- App data folder — `settings.json` (base URL, model), `recent_repos.json`
- OS credential store — API key; never written to a file or sent to the frontend
- Cache key (`store::cache_key`): `git:<HEAD>`; uncommitted or untracked project changes → `git:<HEAD>+fp:<hash>`; no git → `fp:<hash of paths, sizes, mtimes>`
- Stored key differs from the current key → modules shown with an outdated badge until regenerated
## LLM and privacy
- One client for cloud or local OpenAI-compatible servers; settings: base URL, model, optional API key
- Only structure or doc text goes to the LLM; source leaves the machine only if the base URL points to a cloud LLM
- Doc text, structure input and the overview are untrusted: prompts wrap them in `<<<...>>>` delimiters and mark them as data, not instructions
- `git` runs with `core.fsmonitor=false` and no optional locks, since the repo may be untrusted
## Invariants
- One generation at a time: `RunGuard` in `modules.rs`, shared by module and overview generation; one `CANCEL` flag, set by `cancel_generation`
- Every project file has exactly one primary module
- File access is read-only and limited to the open repo
- One repo open at a time
## Later (not MVP)
- Over-engineering and AI-slop detection, suggested updates
- Repo overview as context for module generation and analysis; not wired yet
- Batch split for large projects; deterministic clustering before the LLM
- SQLite if analysis findings need queries; macOS build
