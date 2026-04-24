# Changelog

All notable changes to this Zed extension are listed here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project follows [Semantic Versioning](https://semver.org/).

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
