// cc-plugins: Plugin runtime for the Claude Code CLI Rust port.
//
// This crate handles plugin discovery, manifest parsing, hook registration,
// and the /plugin + /reload-plugins command definitions.
//
// Dependency order: cc-plugins → cc-core only.
// cc-commands → cc-plugins (not the other way around).

pub mod hooks;
pub mod loader;
pub mod manifest;
pub mod plugin;
pub mod registry;

// Re-export the most commonly used items at the crate root.
pub use hooks::{HookOutcome, HookRegistry, RegisteredHook, register_plugin_hooks};
pub use loader::{default_user_plugins_dir, discover_plugins, project_plugins_dir};
pub use manifest::{
    HookEventKind, PluginAuthor, PluginHookEntry, PluginHookMatcher, PluginHooksConfig,
    PluginLspServer, PluginManifest, PluginMcpServer, UserConfigValueType,
};
pub use plugin::{
    CommandRunAction, LoadedPlugin, PluginCommandDef, PluginError, PluginSource, ReloadDiff,
};
pub use registry::PluginRegistry;

use std::path::Path;

// ---------------------------------------------------------------------------
// Top-level async API (called from cc-commands / cc-cli)
// ---------------------------------------------------------------------------

/// Discover and load all plugins from the standard locations.
///
/// Search order:
/// 1. `~/.claude/plugins/`  (user-global)
/// 2. `<project_dir>/.claude/plugins/`  (project-local)
/// 3. Any paths listed in `extra_paths`
///
/// Returns a fully populated `PluginRegistry`.  Errors encountered during
/// loading are stored in `registry.errors` rather than propagated, so the
/// caller always gets a usable registry even when individual plugins fail.
pub async fn load_plugins(
    project_dir: &Path,
    extra_paths: &[std::path::PathBuf],
) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    let mut search_dirs: Vec<std::path::PathBuf> = Vec::new();

    // 1. User-global plugins directory.
    if let Some(user_dir) = default_user_plugins_dir() {
        search_dirs.push(user_dir);
    }

    // 2. Project-local plugins directory.
    search_dirs.push(project_plugins_dir(project_dir));

    // 3. Extra paths (from --plugin-dir or settings).
    search_dirs.extend_from_slice(extra_paths);

    // User plugins.
    if let Some(user_dir) = default_user_plugins_dir() {
        let (plugins, errors) = discover_plugins(&[user_dir], PluginSource::User).await;
        registry.extend(plugins, errors);
    }

    // Project plugins.
    let proj_dir = project_plugins_dir(project_dir);
    let (plugins, errors) = discover_plugins(&[proj_dir], PluginSource::Project).await;
    registry.extend(plugins, errors);

    // Extra paths.
    for path in extra_paths {
        let (plugins, errors) =
            discover_plugins(&[path.clone()], PluginSource::Extra(path.to_string_lossy().into_owned())).await;
        registry.extend(plugins, errors);
    }

    registry
}

/// Reload plugins: produce a new registry, compute the diff, and replace the old one.
///
/// Returns the new registry and a `ReloadDiff` describing what changed.
pub async fn reload_plugins(
    old_registry: &PluginRegistry,
    project_dir: &Path,
    extra_paths: &[std::path::PathBuf],
) -> (PluginRegistry, ReloadDiff) {
    let new_registry = load_plugins(project_dir, extra_paths).await;
    let diff = new_registry.diff_against(old_registry);
    (new_registry, diff)
}

// ---------------------------------------------------------------------------
// /plugin command definition (data-only, no SlashCommand impl here)
// ---------------------------------------------------------------------------

/// Sub-commands supported by `/plugin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSubCommand {
    /// `/plugin list` — show all installed plugins with enabled/disabled status.
    List,
    /// `/plugin enable <name>` — enable a plugin.
    Enable(String),
    /// `/plugin disable <name>` — disable a plugin.
    Disable(String),
    /// `/plugin info <name>` — show details about a plugin.
    Info(String),
    /// `/plugin install <path>` — install a plugin from a local path.
    Install(String),
    /// Show usage / help.
    Help,
}

/// Parse the arguments string for `/plugin`.
pub fn parse_plugin_args(args: &str) -> PluginSubCommand {
    let parts: Vec<&str> = args.trim().splitn(3, char::is_whitespace).collect();
    match parts.first().map(|s| s.to_lowercase()).as_deref() {
        Some("list") | Some("ls") => PluginSubCommand::List,
        Some("enable") => PluginSubCommand::Enable(
            parts.get(1).unwrap_or(&"").to_string(),
        ),
        Some("disable") => PluginSubCommand::Disable(
            parts.get(1).unwrap_or(&"").to_string(),
        ),
        Some("info") | Some("show") => PluginSubCommand::Info(
            parts.get(1).unwrap_or(&"").to_string(),
        ),
        Some("install") | Some("i") => PluginSubCommand::Install(
            parts.get(1).unwrap_or(&"").to_string(),
        ),
        Some("help") | Some("--help") | Some("-h") => PluginSubCommand::Help,
        _ => PluginSubCommand::Help,
    }
}

/// Build the text output for `/plugin list`.
pub fn format_plugin_list(registry: &PluginRegistry) -> String {
    let mut out = String::new();
    let mut all: Vec<&LoadedPlugin> = registry.all();
    all.sort_by(|a, b| a.name.cmp(&b.name));

    if all.is_empty() {
        return "No plugins installed.\n\nUse `/plugin install <path>` to install a plugin from a local directory.".to_string();
    }

    out.push_str("Installed plugins:\n\n");
    for p in &all {
        let status = if registry.is_enabled(&p.name) {
            "enabled"
        } else {
            "disabled"
        };
        let version = p.manifest.version.as_deref().unwrap_or("(no version)");
        let desc = p.manifest.description.as_deref().unwrap_or("");
        out.push_str(&format!("  {} [{}] v{}", p.name, status, version));
        if !desc.is_empty() {
            out.push_str(&format!(" — {}", desc));
        }
        out.push('\n');
    }
    out
}

/// Build the text output for `/plugin info <name>`.
pub fn format_plugin_info(registry: &PluginRegistry, name: &str) -> String {
    match registry.get(name) {
        None => format!("Plugin '{}' not found. Use `/plugin list` to see installed plugins.", name),
        Some(p) => {
            let mut out = String::new();
            out.push_str(&format!("Plugin: {}\n", p.name));
            if let Some(v) = &p.manifest.version {
                out.push_str(&format!("Version: {}\n", v));
            }
            if let Some(d) = &p.manifest.description {
                out.push_str(&format!("Description: {}\n", d));
            }
            if let Some(author) = &p.manifest.author {
                out.push_str(&format!("Author: {}\n", author.name));
            }
            out.push_str(&format!(
                "Status: {}\n",
                if registry.is_enabled(name) { "enabled" } else { "disabled" }
            ));
            out.push_str(&format!("Source: {}\n", p.source_id));
            out.push_str(&format!("Path: {}\n", p.path.display()));

            // Count commands.
            let cmd_defs = loader::collect_command_defs(p);
            if !cmd_defs.is_empty() {
                out.push_str(&format!("\nCommands ({}):\n", cmd_defs.len()));
                for cmd in &cmd_defs {
                    out.push_str(&format!("  /{} — {}\n", cmd.name, cmd.description));
                }
            }

            // MCP servers.
            if !p.manifest.mcp_servers.is_empty() {
                out.push_str(&format!("\nMCP servers ({}):\n", p.manifest.mcp_servers.len()));
                for srv in &p.manifest.mcp_servers {
                    out.push_str(&format!("  {}\n", srv.name));
                }
            }

            // LSP servers.
            if !p.manifest.lsp_servers.is_empty() {
                out.push_str(&format!("\nLSP servers ({}):\n", p.manifest.lsp_servers.len()));
                for srv in &p.manifest.lsp_servers {
                    out.push_str(&format!("  {}\n", srv.name));
                }
            }

            out
        }
    }
}

/// Install a plugin from a local path.
///
/// Copies the plugin directory into `~/.claude/plugins/` and returns the
/// loaded plugin name on success.
pub fn install_plugin_from_path(
    source_path: &Path,
) -> Result<String, PluginError> {
    // Validate that the source looks like a plugin directory.
    if !source_path.exists() {
        return Err(PluginError::Io {
            path: source_path.to_string_lossy().into_owned(),
            message: "Path does not exist".to_string(),
        });
    }

    let manifest_path = if source_path.join("plugin.json").exists() {
        source_path.join("plugin.json")
    } else if source_path.join("plugin.toml").exists() {
        source_path.join("plugin.toml")
    } else {
        return Err(PluginError::InvalidManifest {
            path: source_path.to_string_lossy().into_owned(),
            message: "No plugin.json or plugin.toml found in directory".to_string(),
        });
    };

    let bytes = std::fs::read(&manifest_path).map_err(|e| PluginError::Io {
        path: manifest_path.to_string_lossy().into_owned(),
        message: e.to_string(),
    })?;

    let manifest = if manifest_path.extension().map(|e| e == "toml").unwrap_or(false) {
        PluginManifest::from_toml(&bytes)
    } else {
        PluginManifest::from_json(&bytes)
    }
    .map_err(|e| PluginError::InvalidManifest {
        path: manifest_path.to_string_lossy().into_owned(),
        message: e.to_string(),
    })?;

    let plugin_name = manifest.name.clone();

    // Determine install destination.
    let dest = match default_user_plugins_dir() {
        Some(d) => d.join(&plugin_name),
        None => {
            return Err(PluginError::Io {
                path: String::new(),
                message: "Cannot determine home directory for plugin installation".to_string(),
            })
        }
    };

    // Copy source to dest.
    copy_dir_all(source_path, &dest).map_err(|e| PluginError::Io {
        path: dest.to_string_lossy().into_owned(),
        message: e.to_string(),
    })?;

    tracing::info!(
        "Installed plugin '{}' from '{}' to '{}'",
        plugin_name,
        source_path.display(),
        dest.display()
    );

    Ok(plugin_name)
}

/// Recursively copy a directory.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// /reload-plugins summary formatting
// ---------------------------------------------------------------------------

/// Format the result of a plugin reload into a human-readable string,
/// suitable for the `/reload-plugins` command output.
pub fn format_reload_summary(
    registry: &PluginRegistry,
    diff: &ReloadDiff,
) -> String {
    let enabled = registry.enabled_count();
    let total = registry.plugin_count();

    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "{} plugin{} loaded ({} enabled)",
        total,
        if total == 1 { "" } else { "s" },
        enabled
    ));

    let cmd_count: usize = registry.all_command_defs().len();
    parts.push(format!(
        "{} command{}",
        cmd_count,
        if cmd_count == 1 { "" } else { "s" }
    ));

    let hook_count: usize = registry.build_hook_registry().values().map(|v| v.len()).sum();
    parts.push(format!(
        "{} hook{}",
        hook_count,
        if hook_count == 1 { "" } else { "s" }
    ));

    let mcp_count = registry.all_mcp_servers().len();
    parts.push(format!(
        "{} plugin MCP server{}",
        mcp_count,
        if mcp_count == 1 { "" } else { "s" }
    ));

    let lsp_count = registry.all_lsp_servers().len();
    parts.push(format!(
        "{} plugin LSP server{}",
        lsp_count,
        if lsp_count == 1 { "" } else { "s" }
    ));

    let mut msg = format!("Reloaded: {}", parts.join(" · "));

    if !diff.added.is_empty() {
        msg.push_str(&format!("\nAdded: {}", diff.added.join(", ")));
    }
    if !diff.removed.is_empty() {
        msg.push_str(&format!("\nRemoved: {}", diff.removed.join(", ")));
    }
    if !diff.updated.is_empty() {
        msg.push_str(&format!("\nUpdated: {}", diff.updated.join(", ")));
    }
    if diff.error_count > 0 {
        msg.push_str(&format!(
            "\n{} error{} during load.",
            diff.error_count,
            if diff.error_count == 1 { "" } else { "s" }
        ));
    }

    msg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, json: &serde_json::Value) {
        let path = dir.join("plugin.json");
        std::fs::write(path, serde_json::to_vec_pretty(json).unwrap()).unwrap();
    }

    #[test]
    fn parse_plugin_args_list() {
        assert_eq!(parse_plugin_args("list"), PluginSubCommand::List);
        assert_eq!(parse_plugin_args("ls"), PluginSubCommand::List);
    }

    #[test]
    fn parse_plugin_args_enable() {
        assert_eq!(
            parse_plugin_args("enable my-plugin"),
            PluginSubCommand::Enable("my-plugin".to_string())
        );
    }

    #[test]
    fn parse_plugin_args_info() {
        assert_eq!(
            parse_plugin_args("info my-plugin"),
            PluginSubCommand::Info("my-plugin".to_string())
        );
    }

    #[tokio::test]
    async fn load_plugins_empty_dirs() {
        let tmp = TempDir::new().unwrap();
        let reg = load_plugins(tmp.path(), &[]).await;
        assert_eq!(reg.plugin_count(), 0);
        assert_eq!(reg.error_count(), 0);
    }

    #[tokio::test]
    async fn load_plugins_finds_project_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join(".claude").join("plugins").join("test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            &serde_json::json!({ "name": "test-plugin", "version": "1.0.0", "description": "A test plugin" }),
        );

        let reg = load_plugins(tmp.path(), &[]).await;
        assert_eq!(reg.plugin_count(), 1);
        assert!(reg.get("test-plugin").is_some());
        assert!(reg.is_enabled("test-plugin"));
    }

    #[test]
    fn manifest_parse_json() {
        let json = serde_json::json!({
            "name": "my-plugin",
            "version": "0.1.0",
            "description": "Test",
            "mcpServers": {
                "my-server": { "command": "node", "args": ["server.js"] }
            }
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let manifest = PluginManifest::from_json(&bytes).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.mcp_servers.len(), 1);
        assert_eq!(manifest.mcp_servers[0].name, "my-server");
    }

    #[test]
    fn format_plugin_list_empty() {
        let reg = PluginRegistry::new();
        let out = format_plugin_list(&reg);
        assert!(out.contains("No plugins installed"));
    }

    #[test]
    fn format_reload_summary_basic() {
        let reg = PluginRegistry::new();
        let diff = ReloadDiff::default();
        let out = format_reload_summary(&reg, &diff);
        assert!(out.contains("Reloaded"));
    }
}
