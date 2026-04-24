//! `nanos-world-lua` Zed extension: a thin LuaLS wrapper that auto-injects
//! the bundled nanos world EmmyLua annotations into `Lua.workspace.library`.
//!
//! Architecture:
//!
//! - The annotations file (`library/nanos-world.lua`) is embedded into the
//!   WebAssembly binary at compile time via `include_str!`. This keeps the
//!   extension offline-installable and reproducible.
//! - On first LSP launch, the embedded content is written to
//!   `nanos-world-annotations.lua` inside the extension's work directory,
//!   and the absolute path is cached.
//! - `language_server_command` resolves the `lua-language-server` binary in
//!   this order: user-configured `lsp.nanos-world-lua.binary.path` →
//!   `lua-language-server` on PATH → Zed-managed download from
//!   `LuaLS/lua-language-server` GitHub releases.
//! - `language_server_workspace_configuration` merges any user-provided
//!   `lsp.nanos-world-lua.settings` on top of our defaults (`Lua.workspace
//!   .library` includes the annotations file, `Lua.workspace.checkThirdParty
//!   = false`, `Lua.runtime.version = "Lua 5.4"`).

use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};
use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

/// The generated nanos world EmmyLua annotations bundle.
///
/// Sourced from the `docgen-output` branch of
/// `nanos-world/vscode-extension`. See `.github/workflows/sync-annotations.yml`
/// for the mirror job. If this file is still the in-tree placeholder, the
/// extension will function but completions will be thin — replace it with the
/// real file before building a release (see README).
const BUNDLED_ANNOTATIONS: &str = include_str!("../library/nanos-world.lua");

/// Filename we write the annotations to, inside the extension work directory.
const ANNOTATIONS_FILENAME: &str = "nanos-world-annotations.lua";

struct NanosWorldLuaExtension {
    cached_binary_path: Option<String>,
    cached_annotations_path: Option<String>,
}

impl NanosWorldLuaExtension {
    /// Write the embedded annotations to disk (if not already present for
    /// this session) and return an absolute path to the file.
    fn ensure_annotations_written(&mut self) -> Result<String> {
        if let Some(path) = &self.cached_annotations_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        fs::write(ANNOTATIONS_FILENAME, BUNDLED_ANNOTATIONS)
            .map_err(|e| format!("failed to write bundled annotations: {e}"))?;

        let cwd = std::env::current_dir()
            .map_err(|e| format!("failed to resolve extension work directory: {e}"))?;
        let absolute: PathBuf = cwd.join(ANNOTATIONS_FILENAME);
        let path = absolute.to_string_lossy().into_owned();

        self.cached_annotations_path = Some(path.clone());
        Ok(path)
    }

    /// Resolve the `lua-language-server` binary to launch.
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Ok(settings) = LspSettings::for_worktree(language_server_id.as_ref(), worktree) {
            if let Some(binary) = settings.binary {
                if let Some(path) = binary.path {
                    return Ok(path);
                }
            }
        }

        if let Some(path) = worktree.which("lua-language-server") {
            return Ok(path);
        }

        self.zed_managed_binary_path(language_server_id)
    }

    /// Download and cache `lua-language-server` from GitHub releases. Mirrors
    /// the strategy used by the community `zed-extensions/lua` extension.
    fn zed_managed_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "LuaLS/lua-language-server",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "lua-language-server-{version}-{os}-{arch}.{extension}",
            version = release.version,
            os = match platform {
                zed::Os::Mac => "darwin",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "win32",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X8664 => "x64",
                zed::Architecture::X86 => return Err("unsupported platform x86".into()),
            },
            extension = match platform {
                zed::Os::Mac | zed::Os::Linux => "tar.gz",
                zed::Os::Windows => "zip",
            },
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no GitHub asset matched {asset_name:?}"))?;

        let version_dir = format!("lua-language-server-{}", release.version);
        let binary_path = format!(
            "{version_dir}/bin/lua-language-server{ext}",
            ext = match platform {
                zed::Os::Mac | zed::Os::Linux => "",
                zed::Os::Windows => ".exe",
            },
        );

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                match platform {
                    zed::Os::Mac | zed::Os::Linux => zed::DownloadedFileType::GzipTar,
                    zed::Os::Windows => zed::DownloadedFileType::Zip,
                },
            )
            .map_err(|e| format!("failed to download lua-language-server: {e}"))?;

            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with("lua-language-server-") && name != version_dir {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }

    /// Build the merged LuaLS workspace configuration that will be sent to
    /// the server via `workspace/configuration` responses. User overrides in
    /// `lsp.nanos-world-lua.settings` take precedence where both define the
    /// same key, except that we always additively include our bundled
    /// annotations path in `Lua.workspace.library`.
    fn build_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        let annotations_path = self.ensure_annotations_written()?;

        let user_settings = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings.clone())
            .unwrap_or(Value::Object(Map::new()));

        let mut root = match user_settings {
            Value::Object(map) => map,
            _ => Map::new(),
        };

        let lua = root
            .entry("Lua".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let lua_map = lua
            .as_object_mut()
            .ok_or_else(|| "Lua must be an object".to_string())?;

        // workspace.library: additively include our bundled annotations.
        let workspace = lua_map
            .entry("workspace".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let workspace_map = workspace
            .as_object_mut()
            .ok_or_else(|| "Lua.workspace must be an object".to_string())?;

        let library = workspace_map
            .entry("library".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let library_arr = library
            .as_array_mut()
            .ok_or_else(|| "Lua.workspace.library must be an array".to_string())?;
        if !library_arr
            .iter()
            .any(|v| v.as_str() == Some(&annotations_path))
        {
            library_arr.push(Value::String(annotations_path));
        }

        // Sensible defaults that the user can still override.
        workspace_map
            .entry("checkThirdParty".to_string())
            .or_insert(Value::Bool(false));

        let runtime = lua_map
            .entry("runtime".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(runtime_map) = runtime.as_object_mut() {
            runtime_map
                .entry("version".to_string())
                .or_insert_with(|| Value::String("Lua 5.4".to_string()));
        }

        // Turn inlay hints on at the LuaLS side. Zed still needs
        // `"inlay_hints": { "enabled": true }` in user settings to actually
        // render them, but there's nothing we can do about that from here.
        let hint = lua_map
            .entry("hint".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(hint_map) = hint.as_object_mut() {
            hint_map
                .entry("enable".to_string())
                .or_insert(Value::Bool(true));
            hint_map
                .entry("paramName".to_string())
                .or_insert_with(|| Value::String("All".to_string()));
            hint_map
                .entry("paramType".to_string())
                .or_insert(Value::Bool(true));
            hint_map
                .entry("setType".to_string())
                .or_insert(Value::Bool(false));
            hint_map
                .entry("arrayIndex".to_string())
                .or_insert_with(|| Value::String("Auto".to_string()));
            hint_map
                .entry("await".to_string())
                .or_insert(Value::Bool(true));
        }

        // Belt-and-braces: signatureHelp is on by default in LuaLS, but make
        // it explicit so the feature is resilient to upstream default flips.
        let signature_help = lua_map
            .entry("signatureHelp".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(sig_map) = signature_help.as_object_mut() {
            sig_map
                .entry("enable".to_string())
                .or_insert(Value::Bool(true));
        }

        Ok(Some(Value::Object(root)))
    }
}

impl zed::Extension for NanosWorldLuaExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
            cached_annotations_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let path = self.language_server_binary_path(language_server_id, worktree)?;
        let args = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.binary.and_then(|b| b.arguments))
            .unwrap_or_default();
        Ok(zed::Command {
            command: path,
            args,
            env: Vec::new(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        Ok(LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.initialization_options.clone()))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        self.build_workspace_configuration(server_id, worktree)
    }
}

zed::register_extension!(NanosWorldLuaExtension);
