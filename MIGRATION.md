# MIGRATION.md — porting `nanos-world/vscode-extension` to Zed

This document records the audit of the upstream VS Code extension and the
mapping onto Zed's current extension model. It is deliberately honest about
what was and was not ported, and why.

## 1. Source repo audit

The upstream repository has three relevant branches.

### 1.1 `master` — the published VS Code extension

`package.json` contributions:

- `"activationEvents": ["onLanguage:lua"]`
- `"contributes": {}`  ← **no static VS Code contributions** (no commands, no
  snippets, no grammars, no language configs, no settings schema, no views).
- `"extensionDependencies": ["sumneko.lua"]` — depends on the
  [sumneko/LuaLS VS Code extension](https://github.com/LuaLS/vscode-lua).
- Runtime deps: `axios`.

`src/extension.ts` (the only runtime code) does exactly four things:

1. On activation, creates an `EmmyLua/` directory inside the extension path.
2. HTTP-downloads `annotations.lua` from
   `https://raw.githubusercontent.com/nanos-world/vscode-extension/docgen-output/annotations.lua`.
3. Writes it to `EmmyLua/annotations.lua`.
4. Updates the user's `Lua.workspace.library` setting so LuaLS picks up the
   directory as a library.

The `deactivate` hook attempts to remove itself but its own code comment admits
this "doesn't actually do anything" because VS Code does not let you mutate
settings from `deactivate` / on uninstall.

### 1.2 `docgen` — the generator (default branch)

A TypeScript program run as a GitHub Action that:

- Fetches the nanos world API JSON documentation tree from
  `nanos-world/api` via the GitHub API.
- Walks `Classes/`, `StaticClasses/`, `UtilityClasses/`, `Structs/`, and
  `Enums.json`.
- Emits a single `docs/annotations.lua` file consisting of `---@meta`-tagged
  EmmyLua annotations: `---@class`, `---@field`, `---@param`, `---@return`,
  `---@enum`, `---@operator`, `---@overload`, with `[Server Side]` /
  `[Client Side]` / `[Authority Side]` authority tags and doc-site links in
  the docstrings.

### 1.3 `docgen-output` — the generator artifact

A single-file branch containing the latest `annotations.lua` (~720 KB) produced
by the `docgen` workflow. This is the file the `master` extension downloads at
activation time.

### 1.4 Feature inventory

| Category                              | Present upstream?                                                   |
| ------------------------------------- | ------------------------------------------------------------------- |
| Language registration                 | No — relies on VS Code's built-in Lua language                      |
| Syntax / grammar                      | No                                                                  |
| Snippets                              | No                                                                  |
| Lua language server integration       | Yes, **indirect** — via sumneko's extension + `Lua.workspace.library` |
| Generated API stubs                   | **Yes, this is the core** (`docgen-output/annotations.lua`)         |
| Commands / tasks                      | No                                                                  |
| Settings schema                       | No                                                                  |
| Docs / hovers / completions / diagnostics | Delivered entirely by LuaLS, fed by the annotations file        |
| Repository tooling / generation scripts | Yes — the `docgen` branch (GitHub Action)                        |
| VS Code-only UX pieces                | None                                                                 |

**Conclusion of the audit**: this is a Lua-stubs-provider extension with a
one-line integration into another extension's language server. There is no
custom UI, no custom commands, and no custom behaviour beyond writing a file
and mutating a settings key.

## 2. Zed capability summary

Relevant facts about Zed's current extension model:

- Extensions are Git repos with an `extension.toml` manifest.
- An extension can contribute any subset of: **languages**, **grammars**,
  **language servers**, **themes / icon themes**, **snippets**, **debuggers**,
  **MCP servers**, and **slash commands**. Source of truth:
  [Developing Extensions](https://zed.dev/docs/extensions/developing-extensions).
- Procedural logic (language server launch, init options, workspace config,
  completion relabeling, etc.) is Rust compiled to WebAssembly via
  `zed_extension_api`.
- Language server settings are injected via `language_server_initialization_options`
  and `language_server_workspace_configuration`, but only for language servers
  **that the extension itself owns**. You cannot mutate another extension's LSP
  config from WASM.
- User-level LSP settings live under `lsp.<server_name>.settings.*` and
  `lsp.<server_name>.initialization_options.*` in Zed `settings.json` and
  `.zed/settings.json`.
- The community [`zed-extensions/lua`](https://github.com/zed-extensions/lua)
  extension already contributes `lua-language-server` (registered as `LuaLS`)
  for the `Lua` and `EmmyLuadoc` languages, and forwards both
  `initialization_options` and `settings` from the user's Zed config straight
  to the server.
- Extensions are sandboxed; capabilities like network downloads (`download_file`)
  and process exec (`process:exec`) must be declared and can be restricted by
  the user.
- Dev install is via `zed: install dev extension`.
- Publishing requires a license file (MIT / Apache 2.0 / BSD-style) and a PR
  into [`zed-industries/extensions`](https://github.com/zed-industries/extensions).

## 3. Migration matrix

| # | Source feature                                    | Upstream implementation                                                                               | Zed equivalent                                                                                    | Porting strategy                                                                                                                | Complexity | Status                              |
|---|---------------------------------------------------|-------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------|------------|-------------------------------------|
| 1 | Ship `annotations.lua` to LuaLS                   | Runtime HTTP download to `EmmyLua/annotations.lua` inside the extension directory                     | Ship the same file as `library/nanos-world.lua` inside the Zed extension package                  | Mirror `docgen-output/annotations.lua` into `library/` via a CI workflow. Bundle rather than runtime-download.                   | Low        | Directly portable                   |
| 2 | Register the library with the Lua LSP             | Mutates user's `Lua.workspace.library` via `vscode.workspace.getConfiguration("Lua").update(...)`     | Injected via `language_server_workspace_configuration` on our own `nanos-world-lua` LSP entry      | WASM extension writes bundled annotations to work dir and pushes the absolute path into `Lua.workspace.library` automatically.    | Medium     | Directly portable                   |
| 3 | Dependency on sumneko's VS Code extension          | `extensionDependencies: ["sumneko.lua"]`                                                              | Community `lua` Zed extension (`id = "lua"`) contributes `LuaLS`                                   | Treat it as a soft dependency, documented in README. Zed has no hard extension-to-extension dependency mechanism today.          | Low        | Portable with redesign              |
| 4 | `docgen` generator workflow                       | GitHub Action in the `docgen` branch; produces `annotations.lua` on a schedule                        | N/A — lives entirely upstream                                                                      | Do not re-host. Consume the upstream artifact on a schedule; auto-PR when the file changes.                                      | Low        | Directly portable (as CI sync only) |
| 5 | `deactivate()` cleanup                            | Tries to remove the library path and delete the folder; upstream already notes it is a no-op          | Not needed                                                                                         | Skip. Since we don't mutate settings, there is nothing to clean up on uninstall.                                                 | Low        | Directly portable (as a drop)       |
| 6 | Activation-time HTTP download (`axios`)           | Network request on every activation                                                                    | Possible via `download_file` capability, but requires user-granted network capability              | Avoid. Ship bundled, refresh via CI. Simpler, offline-friendly, reproducible, and removes a runtime failure mode.               | Low        | Portable with redesign              |
| 7 | Commands / tasks                                  | None                                                                                                   | N/A                                                                                               | Nothing to port.                                                                                                                 | —          | Directly portable                   |
| 8 | Snippets                                          | None                                                                                                   | `snippets/lua.json` in Zed                                                                        | **Additive**: ship a small, honest set of nanos-world idioms (Package/Events/Timer). Not in upstream.                            | Low        | Portable with redesign (additive)   |
| 9 | Custom hovers, completions, diagnostics           | None directly; LuaLS provides them based on annotations                                                | Same pattern in Zed (LuaLS via `lua` extension)                                                    | Comes for free once (1) and (2) are in place.                                                                                    | —          | Directly portable                   |
| 10| Automatic, silent settings injection into LuaLS   | Does this on activation in VS Code                                                                     | Only possible for a language server the extension itself owns                                      | **v0.2**: we own `nanos-world-lua`, so injection is fully automatic. The `lua` extension's `LuaLS` is still unreachable from us.  | Medium     | Portable with redesign              |
| 11| Self-host a Lua LSP with the library pre-attached | N/A upstream                                                                                            | WASM extension duplicating `zed-extensions/lua`'s install logic                                    | **v0.2**: implemented. User adds one `"!LuaLS"` override to avoid running two LSPs. Documented in README.                         | Medium     | Directly portable                   |
| 12| Automatic extension dependency install             | VS Code installs `sumneko.lua` automatically via `extensionDependencies`                               | Zed has no equivalent auto-install mechanism today                                                  | Documented as a manual step in README.                                                                                           | —          | Not currently possible in Zed       |

## 4. Recommended architecture

**Chosen (v0.2): Option B — a WASM-backed Zed extension that owns its own
`lua-language-server` instance and auto-injects the bundled annotations.**

Rationale:

- Plug-and-play was an explicit user requirement. The opt-in
  `settings.json` block from v0.1 is correct Zed etiquette but requires
  the user to copy-paste an absolute path, which most players will get
  wrong.
- Owning the LSP lets us call `language_server_workspace_configuration`
  to inject `Lua.workspace.library` directly — the only Zed-native way
  today to guarantee the library path is always correct, regardless of
  where Zed unpacks the extension.
- The binary-install logic is copied almost verbatim from the community
  `zed-extensions/lua` extension (Apache-2.0). No new language server is
  being introduced; we just pin a different workspace configuration for
  the same LuaLS.
- The only remaining user-facing step is a single language-server
  override (`"!LuaLS"`) to avoid running LuaLS twice against the same
  buffer. This is a one-line setting rather than the ten-line
  `settings.json` block required in v0.1.

Rejected alternatives:

- **Option A (snippets-only)** — loses the annotations entirely.
- **Option C (library-only, manual settings)** — v0.1 design. Kept as a
  documented escape hatch for users who want strict upstream parity and
  are happy to wire it in themselves.
- **"Supersede the `lua` extension entirely"** — considered. Would
  require shipping our own tree-sitter grammar and `languages/lua/`
  queries, duplicating upstream work and risking drift every time
  `zed-extensions/lua` updates its queries. Not worth the maintenance
  cost for v0.2.

## 5. MVP definition

The MVP (v0.2) is the smallest thing that is plug-and-play for a nanos world
developer opening a `.lua` file in Zed:

1. Install the community `lua` extension (once, from Zed's marketplace).
   Contributes the Lua tree-sitter grammar.
2. Install this extension. Contributes `nanos-world-lua` — a dedicated
   `lua-language-server` instance with the bundled annotations pre-wired.
3. Add one line to Zed settings to avoid running two LuaLS instances:

   ```json
   { "languages": { "Lua": { "language_servers": ["nanos-world-lua", "!LuaLS"] } } }
   ```

4. Start coding. LuaLS now reports authoritative nanos world completions,
   hovers, parameter hints, doc links, and diagnostics.

Included in the MVP:

- `library/nanos-world.lua` bundled, embedded into the WASM via
  `include_str!`.
- `src/nanos_world_lua.rs` WASM extension implementing `zed::Extension`
  (binary install, annotations write-out, workspace config merge).
- `Cargo.toml` on `zed_extension_api = "0.7"`.
- `snippets/lua.json` with ~10 nanos world idioms.
- `.github/workflows/sync-annotations.yml` mirror of upstream
  `docgen-output`, opening a PR on every change.
- README with install, verification, and advanced-customisation walkthroughs.

Explicitly out of scope for this MVP:

- Shipping our own tree-sitter grammar / superseding the `lua` extension.
- Running without any user settings (Zed does not currently let one
  extension silently win over another for the same language).
- Reimplementing the annotations generator (`docgen`). We consume it, we
  don't re-host it.

## 6. File tree

```
nanos-zed/
├── extension.toml                       # Zed manifest; declares snippets + language_servers.nanos-world-lua
├── Cargo.toml                           # WASM crate, zed_extension_api 0.7
├── LICENSE                              # MIT
├── README.md                            # plug-and-play install flow + advanced tuning
├── MIGRATION.md                         # this document
├── CHANGELOG.md
├── .gitignore
├── src/
│   └── nanos_world_lua.rs               # Rust extension: binary install + annotations inject
├── library/
│   └── nanos-world.lua                  # bundled EmmyLua annotations (placeholder until CI / manual fetch)
├── snippets/
│   └── lua.json                         # small set of nanos-world Lua idioms
└── .github/
    └── workflows/
        └── sync-annotations.yml         # daily mirror of upstream docgen-output
```

Deliberately **not** included:

- `languages/` — we do not register a new language; `Lua` lives in the
  community `lua` extension.
- A tree-sitter grammar — covered by the `lua` extension.
- A GitHub Actions release job — publishing to the Zed registry still
  goes through the `zed-industries/extensions` PR flow (see §J in the
  top-level reply).

## 7. Open blockers and next steps

1. **Double-LSP cohabitation.** With the community `lua` extension and this
   extension both installed, Zed attaches two LuaLS processes to every Lua
   buffer unless the user disables one. The README documents the single
   `"!LuaLS"` language-server override. Blocker on Zed's side: there is no
   mechanism today for an extension to declare it **replaces** another
   extension's language server.
   - Nearest alternative: ship our own tree-sitter grammar and `languages/lua`
     directory so the extension can be used standalone. Explicitly deferred —
     maintenance burden is too high for the MVP.

2. **No automatic extension-to-extension dependency.** Zed has no
   `extensionDependencies` equivalent. The user installs the `lua` extension
   manually. Documented in README.

3. **Bundled 720 KB annotations file** is `include_str!`'d into the WASM,
   so the released WASM grows by roughly the same amount. Acceptable today;
   if upstream grows substantially, move to an on-install `download_file`
   fetch from `raw.githubusercontent.com`.

4. **Cargo.lock.** Not ignored. Must be committed to keep
   `zed-industries/extensions` CI builds reproducible.

5. **Binary duplication on disk.** When both the `lua` extension and this
   one auto-download `lua-language-server`, two copies live on disk (~300 MB
   each). If that becomes a pain point, detect and reuse the sibling
   extension's cache — not currently worth the fragility.

6. **License alignment.** Upstream extension: MIT. This repo: MIT. Binary
   install logic adapted from `zed-extensions/lua` (Apache-2.0) — MIT code
   plus Apache-2.0 snippets is fine as long as the combined work is
   MIT-licensed (which it is) and the Apache-2.0 attribution is kept
   (README "Credits" section).

7. **Versioning.** `extension.toml` and `Cargo.toml` versions must match.
   Bump both on release; the CI mirror job bumps neither and just opens a
   PR with the new annotations so the maintainer can batch.

## 8. Honest caveats

- The snippets in `snippets/lua.json` are **additive** and do not come from
  the upstream extension (which ships none). They are included because they
  are trivial and directly align with the annotations' public surface
  (`Package.Subscribe`, `Events.Subscribe/Call/CallRemote`, `Timer.*`,
  `Character(...)`, `Package.Require`, `Package.Export`). If you prefer strict
  parity with upstream, delete the file and drop the `snippets = [...]` line
  from `extension.toml`.
- The in-tree `library/nanos-world.lua` is a tiny bootstrap placeholder. The
  real file lands on the first CI run or via the curl / Invoke-WebRequest
  one-liner in the README. Dev builds against the placeholder will produce a
  working LSP but with near-empty completions — refresh the file before
  publishing.
- The runtime HTTP download-on-activate from upstream is intentionally
  **not** replicated. Bundling + CI refresh is more reproducible and
  offline-friendly. If you strictly need always-latest stubs without a
  publish cycle, change `BUNDLED_ANNOTATIONS` in
  `src/nanos_world_lua.rs` to a `download_file` call guarded by
  `granted_extension_capabilities`.
