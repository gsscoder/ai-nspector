# AI-nspector

![version](https://img.shields.io/badge/version-0.2.0-blue)

An AI-powered code inspector for desktop. Use it on your own repository or to explore a new one. More and more code is AI-generated, and this app helps you see what is in it and what to improve.

Open a repository. The app finds its projects, asks an LLM to split a project into logical modules, and lets you read the files of each module. Then it reviews a module, or a selection of modules, for over-engineering or bugs, and turns the findings into a prompt that you copy into a coding agent (Claude Code, Codex).

![Demo](docs/demo.gif)

## Why

Tools exist that explain a repository (DeepWiki, Sourcegraph, Greptile) and tools that measure code health (CodeScene, SonarQube). Few tools judge over-engineering and AI-generated "slop". That is the goal of this project. Over-engineering and bug reviews are built; AI-slop detection comes later.

## How it works

```
Repo  →  Project  →  Module  →  File
```

| Level | Source | Notes |
|-------|--------|-------|
| **Repo** | The folder you open | Can hold several independent projects. An optional overview (`brief.md`, `overview.md`) describes the repo to the LLM. |
| **Project** | Manifest files (`package.json`, `.csproj`, `pyproject.toml`, `Cargo.toml`, `go.mod`, `pom.xml`, ...) | No AI. Fast, free, same result every run. Each project gets a role: backend, frontend, test or unknown. |
| **Module** | LLM | The LLM sees only structure: file list, imports and top-level symbols, plus the repo brief. It does not see source code bodies. Show modules as cards or as a graph of their imports. |
| **File** | Source file | Read-only view with syntax highlighting. "Open in editor" opens VS Code. |

- Import and symbol parsing supports **Python**, **C#** and **TypeScript / JavaScript**.
- Every file has exactly one primary module. A file can also be a secondary file of other modules. Files the LLM leaves out go to an "Unassigned" module.
- Module results are cached in `<repo>/.ainspector/` (git-ignored automatically). The cache key is the git commit, plus a fingerprint of local changes. When the repo changes, the app shows an **Outdated** badge. It does not regenerate on its own.

### Analyses

| Analysis | Finds |
|----------|-------|
| **Over-engineering** | Code that can do the same thing more directly: dead code, needless abstraction, pass-throughs, speculative flexibility. |
| **Bugs** | Defects with a concrete failure scenario: security, data, integration, concurrency, performance and more. |

- **Module analysis** runs one analysis on one module.
- **Project analysis** runs one analysis on the modules you select, one module after the other.
- Both need the repo overview. The report shows each module as done, nothing found or failed. It is not saved.
- **Create agent prompt** turns a report into a prompt for a coding agent. The prompt lists the findings as proposals with their confidence and tells the agent to verify each one in the code before it changes anything. Edit it if needed, then copy it.

Both analyses review only. The app never changes your code.

### Privacy

- **Module identification** sends structure only (file paths, imports, symbols) and the repo brief. No source code.
- **Repo overview** sends the text of the documentation files that you select. Do not select files that hold secrets or internal URLs. When a repo has no documentation, you can confirm an overview inferred from the structure: file paths, manifest files and module names. No source code is sent.
- **Module and project analysis send the source code of the module files**, the project structure and the repo brief.

Any OpenAI-compatible endpoint works, and Azure Foundry too. Use a local model (for example Ollama) if your code must not leave your machine. The API key is stored in the OS credential store (Windows Credential Manager, or the Secret Service on Linux). The key is never written to the repo.

## Tech stack

[Tauri 2](https://tauri.app/) (Rust backend), React, TypeScript, Vite, Tailwind CSS, shadcn/ui, [Shiki](https://shiki.style/), [React Flow](https://reactflow.dev/) with dagre, tree-sitter.

## Build and run

### Prerequisites (all platforms)

- [Node.js](https://nodejs.org/) 20 or newer
- [Rust](https://rustup.rs/) (stable, 2024 edition support)
- `git` on the `PATH` (the app uses it for the cache key)
- Optional: [VS Code](https://code.visualstudio.com/) with the `code` command on the `PATH`, for "Open in editor"

### Windows

1. Install the **Visual Studio Build Tools** with the "Desktop development with C++" workload.
2. Install WebView2. Windows 11 has it already. For Windows 10, get the [Evergreen runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
3. Install Rust with the MSVC toolchain (`rustup default stable-msvc`).
4. Then run:

```powershell
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # release build: installer in src-tauri\target\release\bundle\nsis
```

### Linux (Debian / Ubuntu)

```sh
sudo apt install build-essential libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libsecret-1-dev gnome-keyring

npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # release build: .deb and AppImage in src-tauri/target/release/bundle/
```

For other distributions, see the [Tauri prerequisites](https://tauri.app/start/prerequisites/). You need a Secret Service provider (`gnome-keyring` or KWallet). Without one, the app cannot save the API key.

> The first build compiles all Rust dependencies. A cold `tauri build` can take more than 10 minutes.
> Build on the target platform. Cross-building is not supported.

## Use

1. Open **Settings**. Select **OpenAI-compatible** or **Azure Foundry**, enter the base URL, the model (the deployment name on Azure) and the API key. Press **Test connection**. Until the LLM is configured, the AI actions stay disabled.
   - OpenRouter: `https://openrouter.ai/api/v1`
   - Ollama: `http://localhost:11434/v1` (no key needed)
   - Azure Foundry: `https://<resource>.cognitiveservices.azure.com` (key required)
2. Open a repository folder. The app lists the detected projects.
3. Recommended: press **Overview**, select the documentation files (or **Infer from structure** when there are none), then press **Generate**. The app writes `brief.md`, `overview.md` and `overview.json` to `<repo>/.ainspector/`. A new run overwrites them. The overview makes the AI answers more precise, and project analysis needs it.
4. Select a project, then press **Identify modules**. You can cancel while it runs. Switch between **Cards** and **Graph**; in the graph, hover a module to see the modules it is linked to.
5. Select a module, then select a file to read it.
6. Module analysis: in the **Analyses** panel of a module, press **Run** on an analysis. When the report is ready, press **Create agent prompt**.
7. Project analysis: in the **Project analysis** panel of a project, select an analysis and the modules, then press **Run**. When the report is ready, press **Create agent prompt**.

The breadcrumb at the top (`repo › project › module`) has a dropdown on each level to switch between siblings.

## Test

```sh
cd src-tauri && cargo test --lib    # Rust unit tests
npx tsc --noEmit                    # type check the frontend
```

## Known limits

- The prompt budget is fixed at about 100,000 tokens. A very large project gives an error when you identify modules. A module too large for one request cannot be analyzed; in a project analysis, only that module fails. Batch splitting is not built yet.
- TypeScript path aliases (for example `@/`) look like external packages, so those imports are missing from the import graph.
- "Open in editor" works only with VS Code. If it is not found, the app shows the file in the file manager.
- Reports are not saved: closing a report loses it.
- AI-slop detection is planned. It is not built yet.

## License

MIT © [koder0x](https://github.com/gsscoder)
