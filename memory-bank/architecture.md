# ai-nspector architecture
- Audience: AI coding agents; machine consumption, not a human audience
- Use: fast mental model before work in the repo; concepts in `../.agent-files/intent.md`, tech decisions in `../.agent-files/spec.md`
## Purpose
- Desktop app: help a user understand a repo they did not write, often AI-generated
- Navigation: repo → project → module → file; navigation is the frame, AI analysis is the product
- Implemented: project detection, AI module split with a module graph, repo overview, module analysis (over-engineering, bugs) with chat and agent prompt, project analysis over selected modules; AI-slop judgement comes later
- Mission: find improvements in code and turn them into a prompt the user copies into a coding agent (Claude Code, Codex)
## Process model
- Tauri 2, one executable, no sidecar; Windows and Linux
- Rust backend runs all analysis in the app process: walk, parse, LLM calls, cache
- React frontend calls Rust commands through `invoke`; commands are registered in `src-tauri/src/lib.rs`
- Module generation, overview generation, module and project analysis emit progress stages as the `modules-progress` event; analysis also streams answer text as the `chat-delta` event (`{ text }`); project analysis emits `project-analysis-progress` (`{ index, total, moduleName }`, 1-based) before each module
## Backend (`src-tauri/src/`)
- `scan.rs` — repo walk (`ignore` crate, respects `.gitignore`, skips build/vendor dirs and `.ainspector/`), project detection from manifests, doc candidates, recent repos; `open_repo`, `list_recent_repos`
- `parse.rs` + `parse/` (`python.rs`, `csharp.rs`, `script.rs` for TS/JS, `paths.rs`) — tree-sitter imports and top-level symbols per file; syntactic, never runs a language tool chain
- `modules.rs` — module generation: brief + structure prompt, LLM call, validation, cache write; cross-module import counts for the graph; owns the run lock and cancel flag; `load_modules`, `module_relations`, `generate_modules`, `cancel_generation`
- `overview.rs` — repo overview: documented or inferred prompt, 2 LLM calls, validation, atomic write; `load_overview`, `generate_overview`
- `analysis.rs` — module and project analysis: `ANALYSES` catalog (`over-engineering`, `bugs`), context build, streamed chat, project report, agent prompt; `list_analyses`, `load_chat`, `start_analysis`, `start_project_analysis`, `send_chat_message`, `discard_chat`, `create_agent_prompt`, `create_project_agent_prompt`
- `llm.rs` — OpenAI-compatible Chat Completions client (`reqwest`): `complete` (one answer), `stream` (SSE deltas, multi-turn `ChatMessage` list); `test_connection`
- `settings.rs` — provider (`openai`, `azure`), base URL and model in the app data folder, API key in the OS credential store (`keyring`); `get_settings`, `save_settings`
- `store.rs` — `<repo>/.ainspector/` paths, module cache read/write, cache key, chat files (`save_chat`, `load_chat`, `delete_chat`, `delete_project_chats`)
- `files.rs` — `read_file`: read-only, path must resolve inside the open repo, size cap
- `error.rs` — shared `AppError`
## Frontend (`src/`)
- `components/` — screens: `StartScreen`, `RepoScreen` (project cards with a role icon), `RepoView`, `ProjectView`, `ModuleView`, `CodeView` (Shiki), `Settings`, `OverviewDialog`, `Notice` (`info`, `warning`, `error` tones); `components/ui/` is shadcn (Base UI)
  - Project: `ProjectView` toggles module cards and `ModuleGraph` (`@xyflow/react`, `dagre` layout; hover dims unrelated modules; edges from `module_relations`); `ProjectAnalysisPanel` (right column: module selection, analysis, run), `ProjectReportView` (report under the modules, held in state only)
  - Module analysis: `AnalysisPanel` (right column), `ChatPanel` (bottom, sticky), `AgentPromptDialog`, `GenerationProgress`; `lib/useGeneration.ts` holds stage, seconds and error of one run
  - LLM not configured: `lib/llmConfigured.ts` context (`useLlmBlocked`) disables every LLM action, `LlmHint` tells why; unknown state keeps actions enabled
- `api.ts` — typed `invoke` wrappers, progress, project-analysis-progress and chat-delta listeners, open file in editor or reveal in folder (`@tauri-apps/plugin-opener`)
- `settingsApi.ts` — settings wrappers; API key is write-only, the backend returns only `hasApiKey`
- `types.ts` — mirrors the Rust structs (`Repo`, `Project`, `DocFile`, `ModulesFile`, `OverviewInfo`, `AnalysisInfo`, `ChatMessage`, `ChatFile`, `ModuleRelation`, `ProjectReport`, `ModuleReport`, `ProjectAnalysisProgress`)
## Flows
```
open_repo         → scan (manifests, no AI) → Repo { projects, docs }
generate_modules  → scan → parse → brief.md (optional) + structure prompt → LLM → validate (retry once) → .ainspector/modules/
module_relations  → modules cache + parse → cross-module import counts (no AI)
generate_overview → doc texts | structure → LLM: overview.md → validate → LLM: brief.md → .ainspector/
start_analysis    → module source + overview.md + structure → LLM stream → first chat message → .ainspector/chats/
send_chat_message → context rebuilt + stored chat + user text → LLM stream → chat file updated
create_agent_prompt → module name + file paths + chat → LLM → prompt text (returned, not stored)
start_project_analysis → per selected module: module source + brief.md + structure → LLM stream → ProjectReport (returned, not stored)
create_project_agent_prompt → per module: name + file paths + report text → LLM → prompt text (returned, not stored)
```
### Open repo
- Deterministic: a file belongs to the nearest manifest above it; files under no manifest form the `(other files)` project
- Each project gets a `Role` (`backend`, `frontend`, `test`, `unknown`) from manifest content; `test` first, from the folder name (`test`, `tests`, `e2e`, or a `.tests`/`-test`/`_tests`-style suffix) or a .NET test marker in a `.csproj`
- Second walk collects doc candidates (`Repo.docs`), some marked suggested; also enters `.github/`, `.agent-files/`, `.cursor/`
- Adds the repo to recent repos
### Module generation
- Input: `brief.md` first when it exists and is not blank (`<<<BRIEF>>>` block, used only to name and group modules), then structure, one line per file: path, project-internal imports, top-level symbols; no source text
- Over `MAX_PROMPT_TOKENS`: drop symbols; still too large → error; the brief is never dropped and counts in both tries
- Output: JSON; unparsable or no modules → retry once, then error
- Validation repairs the answer: unknown paths dropped, exactly one primary module per file (first claim wins), files left out go to an `Unassigned` module
- `load_modules` reads the cache and computes `outdated`; no automatic LLM call, the user regenerates
- A successful `generate_modules` deletes all chats of the project: module ids can change, old chats would point to wrong files
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
- Exists only when the three files exist; `overview.json` holds `generatedAt` (RFC 3339) and the doc files of a documented run (empty when inferred); no cache key, no outdated state
### Module analysis
- One chat per module; two analyses, both review only: `over-engineering` (based on the `complexity-pruner` agent), `bugs` (defects with a concrete failure scenario: functional, security, data, integration, performance, concurrency, compatibility, UI/UX; empty answer `No defects found.`); a chat must be discarded before another analysis starts
- Module and its file list come from the modules cache by `project_id` + `module_id`, never from the frontend
- Request: system prompt (analysis + chat rules), then a context message: `overview.md` (omitted if absent), project structure, module files sorted by path (secondary files marked `| secondary`; a file imported from outside the module gets `| imported by: <paths>`, sorted, at most 5, then `+N more`, derived from the structure), then the stored chat; the context is rebuilt per call and never stored
- Over `MAX_PROMPT_TOKENS` (context + chat): drop symbols; still too large → error, nothing sent or stored; a long chat can hit this too
- Answer is free text streamed over `chat-delta`; the first answer is the first chat message; cancel or error writes nothing, the chat file stays as before
- Content key: FNV of path and bytes of the module files, sorted; stored in the chat file; `load_chat` computes `outdated` when it differs or a file cannot be read; an outdated chat is read-only (reply and agent prompt refused), the user discards it
- `discard_chat` takes the run lock: fails while a generation runs
- `create_agent_prompt`: one non-streamed call over module name, file paths and the full chat, no source text; the prompt presents the kept findings as proposals with their confidence, tells the coding agent to verify each one in the code first (and to write a failing test first when the repo has tests), and asks it to report applied and skipped findings with the reason; the answer loses one code fence (`overview::unfence`); not stored, the UI holds it
### Project analysis
- One analysis over the modules the user selects in a project; one request per module, one after the other; no chat, no follow-up
- Needs `brief.md`: missing → error before any request; an unknown module id or an unreadable file → error before any request
- Request: same as module analysis, but the system prompt uses project rules (`| secondary` files are context only, no findings on them) and the context has `<<<BRIEF>>>` in place of `<<<OVERVIEW>>>`; no stored chat
- A module too large for one request fails alone; an LLM error fails that module only; the run goes on
- Result `ProjectReport` (analysis id, model, `createdAt`, `cancelled`, modules): each `ModuleReport` is `done`, `nothingFound` (exact empty answer of the analysis) or `failed` (with error); cancel returns the modules finished before
- Not stored: the UI holds the report until closed or a new run
- `create_project_agent_prompt`: the frontend sends `{ moduleId, text }` per module; file paths come from the modules cache; each module is one section with its answer as the single chat message; same `AGENT_PROMPT` as module analysis; no findings → error
## Storage
- `<repo>/.ainspector/` — repo data: `modules/<project-slug>-<hash>.json` (one per project), `chats/<project-slug>-<hash>/<module-slug>-<hash>.json` (one per module: analysis id, module name, content key, model, date, messages), `overview.md`, `brief.md`, `overview.json`
  - On first create writes `.ainspector/.gitignore` with `*`; the repo's own `.gitignore` is never edited
  - Paths inside are relative to the repo root; no secrets
- App data folder — `settings.json` (provider, base URL, model), `recent_repos.json`
- OS credential store — API key; never written to a file or sent to the frontend
- Cache key (`store::cache_key`): `git:<HEAD>`; uncommitted or untracked project changes → `git:<HEAD>+fp:<hash>`; no git → `fp:<hash of paths, sizes, mtimes>`
- Stored key differs from the current key → modules shown with an outdated badge until regenerated
## LLM and privacy
- One client for cloud or local OpenAI-compatible servers; settings: provider, base URL, model, optional API key
  - `azure` (Azure Foundry): base URL reduced to scheme, host and port, then `/openai/v1/chat/completions`; key in the `api-key` header; API key required
- Module and overview generation send structure, `brief.md` or doc text only; module analysis sends the source of the module files, `overview.md` and the structure to the configured LLM; project analysis sends the same per selected module, with `brief.md` in place of `overview.md`; source leaves the machine only if the base URL points to a cloud LLM; no consent dialog, the base URL is the user's choice
- Chat files hold only the visible messages, never a copy of the source
- Doc text, structure input, the overview, the brief, module source and the chat are untrusted: prompts wrap them in `<<<...>>>` delimiters and mark them as data, not instructions
- `git` runs with `core.fsmonitor=false` and no optional locks, since the repo may be untrusted
## Invariants
- One generation at a time: `RunGuard` in `modules.rs`, shared by module generation, overview generation and every analysis command (including `discard_chat`); one `CANCEL` flag, set by `cancel_generation`
- Every project file has exactly one primary module
- File access is read-only and limited to the open repo
- One repo open at a time
## Later (not MVP)
- More analyses (AI-slop, security as its own analysis) and categories in the analysis panel
- Stored project reports
- Batch split for large projects and large modules; deterministic clustering before the LLM; deterministic pre-signals in the analysis prompt
- Markdown rendering and drag resize in the chat panel; consent dialog for cloud LLMs
- SQLite if analysis findings need queries; macOS build
