# ai-nspector intent
- Audience: AI coding agents; machine consumption, not a human audience
- Use: read before building or changing the app; defines why the app exists
- Status: no code yet
## Product
- Name and folder: `ai-nspector`
- Type: desktop app
- Goal: help a user understand a code repository they did not write; more and more often, AI-generated code
- Mature goal: judge code quality; detect over-engineering, detect AI-slop, suggest updates
- Navigation (repo → project → module → file) is the frame only
- AI analysis is the real product and the differentiator
## Market gap
- AI repo understanding exists: DeepWiki, Sourcegraph, Greptile
- Code health exists: CodeScene, SonarQube
- Gap targeted: over-engineering and AI-slop judgement
## Data hierarchy
- Repo → Project → Module → File
### Repo
- Folder the user opens
- Can contain several independent projects, e.g. 2 separate Python projects
- Can be multi-project by design, e.g. a .NET solution
### Project
- Detected deterministically from manifest files; no AI
- Reason: fast, free, same result every run
- Manifests:
  - .NET: `.sln`, `.csproj`
  - Python: `pyproject.toml`, `setup.py`, `requirements.txt`
  - JS/TS: `package.json`, incl. workspaces
  - Rust: `Cargo.toml`
  - Go: `go.mod`
  - Java: `pom.xml`, `build.gradle`
### Module
- Logical part of a project; produced by AI
- AI input: structure, not raw source
  - file tree
  - import graph
  - top-level symbols
  - reason: cheaper; fits large repos
- AI output: JSON; per module: name, short purpose, file list
- Invariant: every file belongs to at least one module
- Optional stabilization: cluster files deterministically first (imports + folders); AI then names and merges clusters
- Cache results by git commit hash
  - reason: LLM output is non-deterministic; unstable modules destroy user trust
### File
- Source file listed under a module
- Actions: view read-only; open in the user's editor
## Navigation (minimal)
- Breadcrumb at top: repo › project › module; each crumb is a dropdown to switch siblings
- Project selected: main area shows module cards; name, purpose, file count; later also analysis findings
- Module selected: single sidebar lists module files; main area shows the selected file
## MVP scope
- Open repo
- Detect projects
- Split a project into modules with AI
- List module files
- Nothing else
## Future (not MVP)
- Over-engineering detection
- AI-slop detection
- Suggested updates
- AI-slop needs measurable signals, e.g.:
  - interface with a single implementation
  - wrapper that only forwards a call
  - comments that restate code
  - duplicate code
  - unused code
  - needless layers
- Static analysis finds many signals; AI adds judgement and suggestions
- Constraint: data kept by the MVP must not block these features
## Risks and constraints
- Privacy: sending source to a cloud LLM is not acceptable for many users → support a local model and/or a user-supplied API key
- Large repos and monorepos: thousands of files → structured AI input required
- Non-deterministic AI output → cache by commit
## Tech decisions
- See `spec.md`