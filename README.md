# nanos world Lua for Zed

A [Zed](https://zed.dev) extension that brings the full nanos world Lua
development experience to Zed, using the same EmmyLua annotation bundle that
powers the [official VS Code extension](https://github.com/nanos-world/vscode-extension).

Unlike a manual LuaLS + library setup, this extension is **plug and play**: it
downloads [`lua-language-server`](https://github.com/LuaLS/lua-language-server)
on first launch, embeds the nanos world API annotations into the WebAssembly
binary, and automatically injects them into the LSP's workspace library. You
don't have to touch `Lua.workspace.library` yourself.

## What you get

- A dedicated Zed language server entry (`nanos-world-lua`) that wraps
  `lua-language-server`.
- Auto-download and version-pinned install of `lua-language-server` via the
  Zed extension API (same mechanism the community `lua` extension uses).
- The full nanos world API as EmmyLua annotations (classes, structs, enums,
  events, operators, constructors), embedded in the WASM and written to the
  extension's work directory on first launch.
- A small set of Lua snippets for common nanos world idioms
  (`nwload`, `nwevent`, `nwtimeout`, …).
- Completion, hovers with `[Server Side]` / `[Client Side]` authority tags,
  doc-site links to `docs.nanos-world.com`, parameter hints, and go-to-
  definition, all provided by LuaLS.

## Requirements

- Zed `0.205.x` or newer (needs `zed_extension_api` 0.7).
- The community [Lua extension for Zed](https://github.com/zed-extensions/lua).
  This extension reuses its tree-sitter grammar for Lua syntax highlighting.
  Install it from the Zed extensions view before installing this one.

## Install

### From the Zed extension marketplace (once published)

1. Open the command palette, run `zed: extensions`, and install the **Lua**
   extension if you don't already have it.
2. Back in the same view, search for **nanos world Lua** and click **Install**.
3. Add the one-line setting in the [Finishing touches](#finishing-touches)
   section and you're done.

### As a dev extension

1. Install Rust via [rustup](https://www.rust-lang.org/tools/install) if you
   don't already have it (Zed requires rustup-managed toolchains for dev
   extensions). Then add the WASM target once:

    ```sh
    rustup target add wasm32-wasip2
    ```

2. Clone this repository.
3. Populate the real annotations file (the in-tree copy is a tiny
   placeholder):

    - macOS / Linux:

       ```sh
       curl -fsSL \
         https://raw.githubusercontent.com/nanos-world/vscode-extension/docgen-output/annotations.lua \
         -o library/nanos-world.lua
       ```

    - Windows (PowerShell):

       ```powershell
       Invoke-WebRequest `
         -Uri "https://raw.githubusercontent.com/nanos-world/vscode-extension/docgen-output/annotations.lua" `
         -OutFile ".\library\nanos-world.lua"
       ```

      The file should be around 700 KB. If it's only a few hundred bytes, you
      got the placeholder.

4. In Zed, run `zed: extensions`, click **Install Dev Extension**, and pick
   the cloned folder. Zed compiles the WASM automatically.

## Finishing touches

When both the `lua` extension and this one are installed, Zed will attach
**two** LuaLS instances to every Lua buffer (the default `LuaLS` from the
`lua` extension, and our `nanos world LuaLS` with the annotations). That works
but doubles the diagnostics. Disable the default one with a single override in
your Zed settings (`zed: open settings`):

```json
{
  "languages": {
    "Lua": {
      "language_servers": ["nanos-world-lua", "!LuaLS"]
    }
  }
}
```

That's the **only** configuration step you need. No `library` paths. No
`workspace` blocks. No absolute paths to copy around.

If you only want this scoped to a specific nanos world project, put the same
block in `.zed/settings.json` at that project's root.

## Verifying the setup

1. Open any `.lua` file in a nanos world package.
2. Type `Package.` — LuaLS should offer `Subscribe`, `Require`, `Export`,
   `Log`, `Warn`, etc., each with the upstream authority tags in the hover
   popup.
3. Hover a class like `Character` or `Weapon` — you should see a link to
   `https://docs.nanos-world.com/docs/scripting-reference/...`.
4. Type `nwload`, `nwevent`, or `nwtimeout` and press tab — the bundled
   snippets should expand.
5. If nothing shows up: open the Zed log (`zed: open log`) and look for
   messages from `nanos world LuaLS`. `zed --foreground` from a terminal
   prints INFO-level logs.

## Advanced: customising LuaLS

Everything in `lsp.nanos-world-lua.settings.*` in your Zed settings is merged
on top of our defaults. The extension guarantees that the nanos world
annotations path stays in `Lua.workspace.library`; everything else is yours.

Example — raise completion snippet detail and silence lowercase-global
warnings:

```json
{
  "lsp": {
    "nanos-world-lua": {
      "settings": {
        "Lua": {
          "completion": { "callSnippet": "Replace" },
          "diagnostics": { "disable": ["lowercase-global"] }
        }
      }
    }
  }
}
```

To pin or override the language server binary:

```json
{
  "lsp": {
    "nanos-world-lua": {
      "binary": {
        "path": "/usr/local/bin/lua-language-server",
        "arguments": []
      }
    }
  }
}
```

If neither `binary.path` nor `lua-language-server` on `PATH` is found, the
extension downloads the latest release from
[`LuaLS/lua-language-server`](https://github.com/LuaLS/lua-language-server)
and caches it.

## Updating the annotations

The annotations are generated upstream on the `docgen` branch of
[`nanos-world/vscode-extension`](https://github.com/nanos-world/vscode-extension)
and published to its `docgen-output` branch.
`.github/workflows/sync-annotations.yml` mirrors that file daily and opens a
pull request whenever it changes; merging the PR + bumping `extension.toml`
and `Cargo.toml` versions is all it takes to ship a new release.

## License

MIT. The underlying annotations are generated from the nanos world public
API documentation. See [`LICENSE`](./LICENSE).

## Credits

- Upstream extension: [nanos-world/vscode-extension](https://github.com/nanos-world/vscode-extension),
  originally by [Derpius](https://github.com/Derpius/), now maintained by
  [nanos world](https://nanos-world.com/).
- Binary-install logic adapted from
  [`zed-extensions/lua`](https://github.com/zed-extensions/lua)
  (Apache-2.0).
- LuaLS: [`LuaLS/lua-language-server`](https://github.com/LuaLS/lua-language-server).
