# ai-nspector architecture
- Audience: AI coding agents; machine consumption, not a human audience
- Use: fast mental model before work in the repo; concepts in `../.agent-files/intent.md`, tech decisions in `../.agent-files/spec.md`
## Purpose
- Desktop app: help a user understand a repo they did not write, often AI-generated
- Navigation: repo → project → module → file; navigation is the frame, AI analysis is the product
- Implemented: project detection, AI module split with a module graph, repo overview, project analysis (over-engineering, bugs) over selected modules, with agent prompt; module analysis is a project analysis over one module; AI-slop judgement comes later
- Mission: find improvements in code and turn them into a prompt the user copies into a coding agent (Claude Code, Codex)
## Process model
- Tauri 2, one executable, no sidecar; Windows and Linux
- Rust backend runs all analysis in the app process: walk, parse, LLM calls, cache
- React frontend calls Rust commands through `invoke`; commands are registered in `src-tauri/src/lib.rs`
- Module generation, overview generation and analysis emit progress stages as the `modules-progress` event (`{ stage }`); every streamed model call sends its answer text as the `chat-delta` event (`{ text }`)
- `prompt-stats` (`{ files, tokens }`, tokens ≈ chars / 4) is sent before the model call of module generation and overview generation, and before each module of an analysis
- Analysis emits `project-analysis-progress` (`{ index, total, moduleName }`, 1-based) before each module
## Backend (`src-tauri/src/`)
- `scan.rs` — repo walk (`ignore` crate, respects `.gitignore`, skips build dirs, `vendor/` only next to a `go.mod`, and `.ainspector/`), project detection from manifests, doc candidates, recent repos; `scan_repo`, `list_recent_repos`
- `parse.rs` + `parse/` (`python.rs`, `csharp.rs`, `script.rs` for TS/JS, `rust.rs`, `go.rs`, `paths.rs`) — tree-sitter imports and top-level symbols per file; syntactic, never runs a language tool chain
  - Go (`tree-sitter-go`): imports are the `import_spec` strings (aliases, blank and dot imports ignored); symbols: functions, methods, struct, interface, other types; an import that starts with the module path of the nearest `go.mod` links to the non-test `.go` files of that folder; external imports give no link; `go::Modules` reads each `go.mod` from disk once per `analyze_project` and builds the folder → files index once; same-package files do not link, `go.work` links, build tags and `replace` lines are not read
- `modules.rs` — module generation: brief + structure prompt, LLM call, validation, cache write; cross-module import counts for the graph; owns the run lock, the cancel flag and the event emitters (`progress`, `delta`, `prompt_stats`); `load_modules`, `module_relations`, `generate_modules`, `cancel_generation`
- `overview.rs` — repo overview: documented or inferred prompt, 2 LLM calls, validation, atomic write; `load_overview`, `generate_overview`
- `analysis.rs` — analysis of selected modules (a module analysis is one selected module): `ANALYSES` catalog (`over-engineering`, `bugs`), request build, streamed answers, project report, agent prompt; `list_analyses`, `start_project_analysis`, `create_project_agent_prompt`
- `llm.rs` — OpenAI-compatible Chat Completions client (`reqwest`): `complete` (one answer), `stream` (SSE deltas, multi-turn `ChatMessage` list); `test_connection`
- `settings.rs` — provider (`openai`, `azure`), base URL and model in the app data folder, API key in the OS credential store (`keyring`); `get_settings`, `save_settings`
- `store.rs` — `<repo>/.ainspector/` paths, module cache read/write, cache key, `git` runner; `open_repo`: `scan_repo`, then `Repo.branch` from `current_branch` (checked-out branch; `None` on a detached HEAD or without git), then recent repos
- `files.rs` — `read_file`: read-only, path must resolve inside the open repo, size cap
- `error.rs` — shared `AppError`
## Frontend (`src/`)
- `components/` — screens: `StartScreen`, `RepoScreen` (project cards with a role icon), `RepoView`, `ProjectView`, `ModuleView`, `CodeView` (Shiki), `Settings`, `OverviewDialog`, `Notice` (`info`, `warning`, `error` tones), `WindowControls` (minimise, maximise, close of the frameless window; renders nothing while the OS draws the frame, as on Linux); `components/ui/` is shadcn (Base UI)
  - Title bar (`App.tsx`): app version, `Repo.branch` when set, Settings button, `WindowControls`
  - Project: `ProjectView` toggles module cards and `ModuleGraph` (`@xyflow/react`, `dagre` layout; hover dims unrelated modules; edges from `module_relations`); `ProjectAnalysisPanel` (right column: module selection, analysis, run), `ProjectReportView` (report under the modules, held in state only)
  - Module analysis: `AnalysisPanel` (right column, a Run button per analysis); `ModuleView` calls `startProjectAnalysis(repoRoot, projectId, analysisId, [module.id])` and shows the result in `ProjectReportView` (`requested` = 1, held in state only)
  - `AgentPromptDialog` (opened from `ProjectReportView`: edit, regenerate, copy); `GenerationProgress` (stage, seconds, last run duration, Cancel; `Sent N files, about Nk tokens` from `prompt-stats`; tokens received; with `kind="modules"` the found module names, with `kind="findings"` the count of numbered findings so far); `lib/useGeneration.ts` listens to `modules-progress`, `chat-delta` and `prompt-stats` and holds stage, seconds, streamed text, prompt stats and error of one run
  - LLM not configured: `lib/llmConfigured.ts` context (`useLlmBlocked`) disables every LLM action, `LlmHint` tells why; unknown state keeps actions enabled
- `api.ts` — typed `invoke` wrappers, progress, project-analysis-progress, chat-delta and prompt-stats listeners, open file in editor or reveal in folder (`@tauri-apps/plugin-opener`)
- `settingsApi.ts` — settings wrappers; API key is write-only, the backend returns only `hasApiKey`
- `types.ts` — mirrors the Rust structs (`Repo`, `Project`, `DocFile`, `ModulesFile`, `OverviewInfo`, `AnalysisInfo`, `ModuleRelation`, `ProjectReport`, `ModuleReport`, `ProjectAnalysisProgress`, `PromptStats`)
## Flows
```
open_repo         → scan (manifests, no AI) + git branch → Repo { projects, docs, branch }
generate_modules  → scan → parse → brief.md (optional) + structure prompt → LLM → validate (retry once) → .ainspector/modules/
module_relations  → modules cache + parse → cross-module import counts (no AI)
generate_overview → doc texts | structure → LLM: overview.md → validate → LLM: brief.md → .ainspector/
start_project_analysis → per selected module (a module analysis: one): module source + brief.md + structure → LLM stream → ProjectReport (returned, not stored)
create_project_agent_prompt → per module: name + file paths + report text → LLM → prompt text (returned, not stored)
```
### Open repo
- Deterministic: a file belongs to the nearest manifest above it; files under no manifest form the `(other files)` project
- Each project gets a `Role` (`backend`, `frontend`, `desktop`, `fullstack`, `library`, `cli`, `fixture`, `test`, `unknown`) from the folder name and manifest content; checked in this order: `fixture` (folder `fixtures`, `testdata`, `examples`, `samples`), `test` (folder `test`, `tests`, `e2e`, or a `.tests`/`-test`/`_tests`-style suffix, or a .NET test marker in a `.csproj`), `library` (`.csproj` with `<OutputType>Library`), then the known frameworks of the manifest: `desktop`, `fullstack` (or frontend plus backend), `frontend`, `backend`, `cli`
- Go: `go.mod` is the manifest, `go.work` is not; name = last segment of the `module` path without a `/vN` suffix (single-line, block form, quoted path, trailing comments handled); `require` lines (indirect skipped, `/vN` stripped) feed framework detection (Gin, Echo, Fiber, Chi, gRPC, Cobra, urfave/cli, Wails, Fyne); the manifest gives no `Library` or `test` role (a `testdata` folder gives Fixture as before)
- Second walk collects doc candidates (`Repo.docs`), some marked suggested; also enters `.github/`, `.agent-files/`, `.cursor/`
- Adds the repo to recent repos
### Module generation
- Input: `brief.md` first when it exists and is not blank (`<<<BRIEF>>>` block, used only to name and group modules), then structure, one line per file: path, project-internal imports, top-level symbols; no source text
- Over `MAX_PROMPT_TOKENS`: drop symbols; still too large → error; the brief is never dropped and counts in both tries
- Output: JSON; unparsable or no modules → retry once, then error
- Validation repairs the answer: unknown paths dropped, exactly one primary module per file (first claim wins), files left out go to an `Unassigned` module
- `load_modules` reads the cache and computes `outdated`; no automatic LLM call, the user regenerates
- The saved `ModulesFile` holds `durationSecs` (seconds from the start of the run to the save; absent in older files)
- `module_relations`: counts file imports between primary modules (`from`, `to`, `count`), sorted; imports inside one module or with a file without a module are skipped; no cache → empty list
### Repo overview
- One per repo, not per project
- Documented mode: text of the user-selected doc files; only paths from `Repo.docs` are accepted
- Inferred mode (`infer = true`, empty `files`): project list, file paths or folder counts, manifest text, module names and purposes from existing caches; no source text
- Call 1 writes `overview.md` with fixed headings; missing headings or no facts (`Not documented`, with or without a period) → retry once; documented mode only: `INSUFFICIENT_SOURCES` sentinel → error, no retry
- Call 2 writes `brief.md` from the overview only
- Inferred result: the code, not the model, writes a source line at the top of `overview.md`
- Before the write, one line-end period is removed from each line of `overview.md` and `brief.md` outside code fences (an ellipsis stays); the prompts also forbid it
- Write: temp files, then rename (`overview.md`, `brief.md`, `overview.json`); on error or cancel the old files stay
- Exists only when the three files exist; `overview.json` holds `generatedAt` (RFC 3339), the doc files of a documented run (empty when inferred) and `durationSecs` (optional); no cache key, no outdated state
### Analysis
- One command, `start_project_analysis`: one analysis over the modules the user selects, one request per module, one after the other; a module analysis is the same call with one module id; no chat, no follow-up, no stored state
- Two analyses, both review only: `over-engineering`, `bugs` (defects with a concrete failure scenario: functional, security, data, integration, performance, concurrency, compatibility, UI/UX)
- Needs `brief.md`: missing → error before any request; an unknown module id or an unreadable module file → error before any request; no modules cache → error
- Module and its file list come from the modules cache by `project_id` + `module_id`, never from the frontend
- Request per module: system prompt (analysis prompt + project rules), then one context message: `<<<BRIEF>>>` (`brief.md`), `<<<STRUCTURE>>>` (project structure), module files sorted by path, each in a `<<<FILE path>>>` block, and the line `analyze this module: <name>`; secondary files are marked `| secondary` and are context only, no findings on them; a file imported from outside the module gets `| imported by: <paths>`, sorted, at most 5, then `+N more`, derived from the structure
- Over `MAX_PROMPT_TOKENS` (system + context, tokens ≈ chars / 4): drop symbols; still too large → error `The module is too large for one request`; that module fails alone, the run goes on; an LLM error fails that module only
- Answer is free text streamed over `chat-delta`; `prompt-stats` and `project-analysis-progress` precede each module call
- Result `ProjectReport` (analysis id, model, `createdAt`, `cancelled`, `durationSecs`, modules): each `ModuleReport` is `done`, `nothingFound` (trimmed answer is exactly `No over-engineering found.` or `No defects found.`) or `failed` (with error, empty text); `durationSecs` is the seconds from the start of the run to the end
- Cancel: the module in flight is dropped; the report has `cancelled = true` and the modules finished before
- Not stored: the UI holds the report until closed or a new run
- `create_project_agent_prompt`: the frontend sends `{ moduleId, text }` per `done` module; file paths come from the modules cache; each module is one section (name, file paths, findings), no source text; no findings → error; one non-streamed call; the prompt presents the findings as proposals with their confidence, tells the coding agent to verify each one in the code first (and to write a failing test first when the repo has tests), and asks it to report applied and skipped findings with the reason; the answer loses one code fence (`overview::unfence`); empty answer → error; not stored, the UI holds it
## Storage
- `<repo>/.ainspector/` — repo data: `modules/<project-slug>-<hash>.json` (one per project), `overview.md`, `brief.md`, `overview.json`
  - On first create writes `.ainspector/.gitignore` with `*`; the repo's own `.gitignore` is never edited
  - Paths inside are relative to the repo root; no secrets
- App data folder — `settings.json` (provider, base URL, model), `recent_repos.json`
- OS credential store — API key; never written to a file or sent to the frontend
- Cache key (`store::cache_key`): `git:<HEAD>`; uncommitted or untracked project changes → `git:<HEAD>+fp:<hash>`; no git → `fp:<hash of paths, sizes, mtimes>`
- Stored key differs from the current key → modules shown with an outdated badge until regenerated
## LLM and privacy
- One client for cloud or local OpenAI-compatible servers; settings: provider, base URL, model, optional API key
  - `azure` (Azure Foundry): base URL reduced to scheme, host and port, then `/openai/v1/chat/completions`; key in the `api-key` header; API key required
- Module and overview generation send structure, `brief.md` or doc text only; analysis sends per selected module (a module analysis: one) the source of the module files, `brief.md` and the structure to the configured LLM; source leaves the machine only if the base URL points to a cloud LLM; no consent dialog, the base URL is the user's choice
- The agent prompt input holds module names, file paths and findings, never source text
- Doc text, structure input, the overview, the brief, module source and the findings are untrusted: prompts wrap them in `<<<...>>>` delimiters and mark them as data, not instructions
- `git` runs with `core.fsmonitor=false` and no optional locks, since the repo may be untrusted
## Invariants
- One generation at a time: `RunGuard` in `modules.rs`, shared by `generate_modules`, `generate_overview`, `start_project_analysis` and `create_project_agent_prompt`; one `CANCEL` flag, set by `cancel_generation`
- Every project file has exactly one primary module
- File access is read-only and limited to the open repo
- One repo open at a time
## Later (not MVP)
- More analyses (AI-slop, security as its own analysis) and categories in the analysis panel
- Stored project reports
- Batch split for large projects and large modules; deterministic clustering before the LLM; deterministic pre-signals in the analysis prompt
- Markdown rendering of reports; consent dialog for cloud LLMs
- SQLite if analysis findings need queries; macOS build
