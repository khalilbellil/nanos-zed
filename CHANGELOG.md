# Changelog

All notable changes to this Zed extension are listed here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project follows [Semantic Versioning](https://semver.org/).

## [0.2.4] - 2026-04-25

### Added

- The four upstream "side-indicator" badge `<img>` tags
  (`both.png`, `client-only.png`, `authority-only.png`,
  `network-authority.png`) are now rewritten to emoji at transform time so
  the side hint actually shows up in Zed's hover popover. Zed's editor
  hover renderer doesn't fetch remote images, so the previous `![](url)`
  rendering produced empty space; the emoji match the docs site's own
  vocabulary:
  - `both.png` → 🟧🟦 (Client & Server)
  - `client-only.png` → 🟧 (Client only)
  - `authority-only.png` → 👑 (Authority side)
  - `network-authority.png` → 🛰️ (Network Authority)
- Match is by URL suffix (`/assets/<name>.png`) rather than full URL, so
  upstream switching from `raw.github.com` to
  `raw.githubusercontent.com` (or moving the asset into a different
  branch) doesn't break the substitution.

### Notes

- Any `<img>` whose `src` doesn't match one of the four known badges
  still falls back to the previous `![](url)` rendering, so future
  upstream additions stay visible (as a hyperlink) instead of being
  silently dropped.

## [0.2.3] - 2026-04-25

### Fixed

- `docs` hover links now point at pages that actually exist on
  `docs.nanos-world.com`. The upstream annotations file emits some URLs
  that 404 on the live site; the in-memory post-processor now repairs them
  before they reach LuaLS:
  - **Multi-word entity classes** (`SceneCapture`, `CharacterSimple`,
    `InstancedStaticMesh`, `WebUI`, `Text3D`, `Widget3D`, `VehicleWheeled`,
    `VehicleWater`, `StaticMesh`, `TextRender`, …) get their slug
    kebab-cased to match `/classes/<kebab>`. The map is built dynamically
    from `---@class` declarations in the bundled file, so newly-added
    upstream classes are picked up automatically.
  - **Utility libraries** (`NanosMath`, `NanosTable`, `NanosUtils`, `JSON`,
    `TOML`) get their category swapped from `/static-classes/` to
    `/utility-libraries/` to match the docs site layout.
  - **Math/value structs** (`Vector`, `Vector2D`, `Color`, `Rotator`,
    `Quat`, `Matrix`) get their category swapped from `/static-classes/`
    to `/structs/` for the same reason.
- Anchor (`#static-function-…`) and query suffixes are preserved across
  rewrites, so deep links keep landing on the right section.

### Notes

- Static classes whose upstream URL is already correct (`PostProcess`,
  `HTTP`, `Events`, `Chat`, `Client`, …) are left untouched.
- `<img>` URLs (the Client/Server/Authority badges hosted on
  `raw.github.com`) and any URL that doesn't point at
  `docs.nanos-world.com` pass through unchanged.

## [0.2.2] - 2026-04-25

### Added

- Hover docstrings now render images, links, bold, italic, inline code and
  list items. The bundled annotations file embeds raw HTML
  (`<img src="…"> <b>[Client/Server Side]</b> <a href="…">docs</a>`,
  `<br>`, `<code>`, `<ul><li>…</li></ul>`, etc.) which Zed's CommonMark
  hover renderer would otherwise print verbatim. The extension now rewrites
  the docstrings to Markdown on first launch, before they reach LuaLS:
  `<b>` → `**bold**`, `<i>` → `*italic*`, `<code>` → `` `code` ``,
  `<br>` → comment-line split, `<img src="…">` → `![](…)`,
  `<a href="…">label</a>` → `[label](…)`. Relative URLs are absolutized
  against `https://docs.nanos-world.com`.
- `src/html_to_markdown.rs` with unit tests for the rewrite rules.

### Notes

- The upstream annotations file on disk is left untouched. The transform
  happens in-memory at LSP-launch time and is written into the extension's
  work directory as `nanos-world-annotations.lua`.
- Unknown HTML tags fall through unchanged, so newly-introduced upstream
  tags won't break the file — they'll just render as raw HTML until the
  transformer learns them.

## [0.2.1] - 2026-04-24

### Added

- Default `Lua.hint.*` settings injected via workspace configuration:
  `hint.enable = true`, `paramName = "All"`, `paramType = true`,
  `setType = false`, `arrayIndex = "Auto"`, `await = true`. Users still
  need to enable Zed's editor-level `"inlay_hints": { "enabled": true }`
  for inline parameter labels to render.
- Explicit `Lua.signatureHelp.enable = true` default (LuaLS already
  defaults to on; kept explicit for resilience to upstream changes).

### Notes

- The Zed-side knobs (`auto_signature_help`, `inlay_hints.enabled`) are
  editor-level and cannot be set from an extension. They live in the
  user's Zed settings.

## [0.2.0] - 2026-04-24

### Added

- **Plug-and-play install flow.** The extension now contributes its own
  `nanos-world-lua` language server (`zed_extension_api` 0.7), auto-downloads
  `lua-language-server` from GitHub releases, and injects the bundled nanos
  world annotations into `Lua.workspace.library` automatically via
  `language_server_workspace_configuration`. Users no longer need to edit
  `Lua.workspace.library` themselves.
- `Cargo.toml` and `src/nanos_world_lua.rs` WASM extension implementation.
- Binary resolution order: user-configured
  `lsp.nanos-world-lua.binary.path` → `lua-language-server` on `PATH` →
  Zed-managed download (same strategy as `zed-extensions/lua`).
- Sensible default LuaLS settings (`Lua.workspace.checkThirdParty = false`,
  `Lua.runtime.version = "Lua 5.4"`) that user settings still override.

### Changed

- `extension.toml` now declares `[language_servers.nanos-world-lua]` and
  bumps `schema_version` metadata. Version → `0.2.0`.
- README rewritten around the plug-and-play flow. The manual-settings
  recipe from v0.1 is replaced by a single `"!LuaLS"` override.

### Deprecated

- The v0.1 manual `lsp.LuaLS.settings.Lua.workspace.library` approach still
  works if used directly against the `lua` extension, but is no longer the
  recommended path; this extension takes care of it automatically.

## [0.1.0] - 2026-04-24

### Added

- Initial Zed port of
  [`nanos-world/vscode-extension`](https://github.com/nanos-world/vscode-extension).
- Ship `library/nanos-world.lua`, mirrored from the upstream `docgen-output`
  branch, as an opt-in `Lua.workspace.library` entry for LuaLS.
- Small set of nanos world idiom snippets at `snippets/lua.json`
  (`nwload`, `nwunload`, `nwevent`, `nwcall`, `nwcallremote`, `nwtimeout`,
  `nwinterval`, `nwchar`, `nwrequire`, `nwexport`).
- Daily `sync-annotations` GitHub Actions workflow that opens a PR when the
  upstream annotations file changes.
