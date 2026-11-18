# Project Overview & AI Instructions

## Project Brief
Desktop app that helps a user understand a code repository they did not write, often AI-generated.
Navigation: repo → project → module → file. AI analysis is the product: it splits each project into logical modules, later it will judge over-engineering and AI-slop.
Concepts in `.agent-files/intent.md`, tech decisions in `.agent-files/spec.md`, implementation plan in `.agent-files/plan.md`.

## Core Technologies
Tauri 2 (Rust 2024, one executable, no sidecar), React 19 + TypeScript + Vite, Tailwind 4 + shadcn/ui (Base UI), Shiki.
Rust crates: `tree-sitter` (Python, C#, TypeScript, JavaScript), `ignore`, `reqwest` + `tokio`, `keyring`.
Target repos: Python, .NET, JS/TS. Platforms: Windows and Linux.

## Layers
- `src-tauri/src/scan.rs` — repo walk (respects `.gitignore`), deterministic project detection from manifest files, recent repos
- `src-tauri/src/parse.rs` and `parse/` — imports and top-level symbols per language with tree-sitter; syntactic, never runs a language tool chain
- `src-tauri/src/modules.rs` — module generation: structure prompt, LLM call, JSON validation, cache
- `src-tauri/src/llm.rs` and `settings.rs` — OpenAI-compatible Chat Completions client; settings in the app data folder, API key only in the OS credential store
- `src-tauri/src/store.rs` and `files.rs` — cache in `<repo>/.ainspector/`; read-only file access limited to the open repo
- `src/components` — screens (start, repo, project, module, settings, code view); `components/ui` is shadcn
- `src/api.ts`, `src/settingsApi.ts`, `src/types.ts` — typed wrappers over Tauri commands; types mirror the Rust structs
- `.agent-files` — intent, spec, plan

## Architecture
Tauri frontend calls Rust commands (`invoke`); all analysis runs in the Rust process:
- Open repo — `scan` finds projects from manifests, no AI
- Parse — `parse` builds per-file imports and symbols for one project
- Generate — `modules` sends structure only (file tree, imports, symbols), never raw source, to the LLM; progress goes to the UI as the `modules-progress` event; the user can cancel
- Validate — every project file gets exactly one primary module; invalid JSON is retried once
- Cache — one JSON file per project in `<repo>/.ainspector/`, keyed by git commit hash; LLM output is non-deterministic, so a cached result is reused until the commit changes
- Privacy — source stays local unless the user points the base URL to a cloud LLM; secrets are never written to a file, logged or sent to the frontend

## Maturity
- The system is currently under development
- Backward compatibility is not required when changing existing features

## Memory
The development documents are in the `memory-bank` dir — they primarily focus on specific feature implementation details

## Output
- Code: match the architectural and stylistic conventions of the existing codebase
- Language: use English for all generated artifacts and symbols by default. Content in another language is allowed only in user-facing strings, messages, and labels when the application has a single localization
- Quality: production-grade — every line will be reviewed
- Markdown: compact, no linting compliance, formatting identical to this file

## Generated Prompts
- Lowercase sentence starts; capitals aren't required at the beginning of a sentence or after a period
- Drop the trailing period when a line break already ends the sentence
- Keep blank lines only where they carry meaning (separating distinct blocks); no filler blank lines
- Favor the tightest clear phrasing; cut redundancy
- Leave already-tight prose alone — don't rewrite what already meets these points

### TypeScript Code Standards
- Avoid `any`; use `unknown` for uncertain types and narrow before use
- Do not use non-null assertion (`!`); handle `null`/`undefined` explicitly
- Avoid `object`/`{}`/`Record<string, any>`; use specific interfaces or type aliases
- Avoid wide unions; use discriminated unions with `type`/`kind` for explicit handling
- Avoid `as` casting; use type guards (`is`) for safe narrowing

### Rust Code Standards
- Avoid `.unwrap()` and `.expect()`; handle errors explicitly using Result and Option with proper propagation (?)
- Never use unsafe unless strictly necessary; isolate it, document its safety invariants, and justify why safe Rust is insufficient
- Avoid clone() and unnecessary allocations; prefer borrowing (`&T`, `&mut T`), references, and zero-cost abstractions
- Avoid `unwrap`-driven control flow and panics; design APIs around explicit error handling with custom error types (`thiserror` / `anyhow` where appropriate)
- Never suppress compiler warnings or use broad lint allowances; enforce idiomatic Rust with `clippy`, `rustfmt`, and strict compiler lints (`#![deny(warnings)`] where appropriate)

## Operational Rules
- For complex requests, start by reading file `memory-bank/architecture.md` before analyzing any code
- Read files in `memory-bank` only when required by the current task; scan filenames first and read file contents only if they are relevant to the task
- Review/audit/report requests end at the report; fixing findings needs its own separate request — authorization never carries across turns
- NEVER update this file
- NEVER modify `*.md` files in `memory-bank` (at any depth in the dir tree) without an explicit request
- NEVER commit changes to Git history without explicit authorization; do not suggest committing changes
 
## Guardrails
Where a request conflicts with a guardrail, follow the guardrail and say why.
 
### Design & Scope — Code Minimally
- Simplicity Over Abstraction: Write the simplest solution that meets the requirements. Avoid speculative features, abstractions for single-use code, unrequested configurability, and error handling for impossible cases. If a construction could be materially shorter without losing correctness, rewrite it — ask whether a senior engineer would call it overcomplicated
- Surgical Scope: Every changed line should trace directly to the current task. Match the surrounding style even where you'd choose differently. Remove imports, variables, and comments that *your* changes made obsolete, but never modify, reformat, or delete code or comments orthogonal to the task. If you notice unrelated dead code, mention it — don't delete it
 
### Execution — Verify Against Goals
- Drive Toward Success Criteria: "fix the bug" → write a failing test that reproduces it, then make it pass