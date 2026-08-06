// Third Party
use anyhow::Result;

// Local
use crate::launchers::LAUNCHER_REGISTRY;
use crate::utils::prompt_from_schema;

/*-- public --*/

pub struct LauncherCommands;

impl LauncherCommands {
    /// Show all launcher types registered in the catalog.
    pub fn catalog(ctx: &crate::AppContext) -> Result<()> {
        let launchers = LAUNCHER_REGISTRY.entries();

        let mut rows: Vec<Vec<String>> = launchers
            .iter()
            .map(|(id, l)| vec![id.to_string(), l.default_command.clone()])
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Launcher Catalog ({} launchers)", launchers.len()),
            &["ID", "DEFAULT COMMAND"],
            &rows,
        );
        Ok(())
    }

    /// List all configured launcher instances.
    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let mut rows: Vec<Vec<String>> = ctx
            .config
            .launchers
            .iter()
            .map(|(id, cfg)| {
                let command = cfg
                    .config
                    .get("command_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(PATH)")
                    .to_string();
                vec![id.clone(), cfg.launcher_type.clone(), command]
            })
            .collect();
        rows.sort_by(|a, b| {
            let type_cmp = a[1].cmp(&b[1]);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a[0].cmp(&b[0])
        });

        ctx.ui.table(
            &format!("Configured Launchers ({} launchers)", rows.len()),
            &["ID", "TYPE", "COMMAND"],
            &rows,
        );
        Ok(())
    }

    /// Interactive launcher setup wizard.
    ///
    /// `launcher_type` is the catalog/registry key (e.g. `claude`).
    /// `instance_id` is the nickname for this instance; defaults to
    /// `launcher_type` when not given.
    ///
    /// **Diverges from Provider setup**: scans all configured launchers for any
    /// entry with the same `launcher_type` — not just the same `instance_id` —
    /// and, if one exists under a different name, offers to either update that
    /// existing entry or proceed with the new name. This lets the user avoid
    /// accidentally creating duplicate configs for the same tool.
    pub async fn setup(
        ctx: &mut crate::AppContext,
        launcher_type: &str,
        instance_id: Option<&str>,
    ) -> Result<()> {
        // Look up type in registry
        let launcher_def = match LAUNCHER_REGISTRY.get(launcher_type) {
            Some(def) => def,
            None => {
                ctx.ui.error(&format!(
                    "Launcher type '{launcher_type}' not found in registry."
                ));
                let available: Vec<String> = {
                    let mut entries: Vec<String> = LAUNCHER_REGISTRY
                        .entries()
                        .iter()
                        .map(|(id, l)| format!("{} ({})", id, l.name))
                        .collect();
                    entries.sort();
                    entries
                };
                ctx.ui
                    .info(&format!("Available types: {}", available.join(", ")));
                anyhow::bail!("Launcher type not found");
            }
        };

        ctx.ui
            .info(&format!("\nSetting up launcher: {launcher_type}"));
        ctx.ui.info(&launcher_def.description);
        ctx.ui.info(&format!(
            "Default command: {} (leave command_path blank to use PATH lookup)",
            launcher_def.default_command
        ));

        // Resolve instance id (prompt only when not passed as arg)
        let instance_id = match instance_id {
            Some(id) => id.to_string(),
            None => ctx.ui.text("Instance name: ", launcher_type)?,
        };

        // --- Type-aware clash detection (diverges from Provider pattern) ---
        // Look for any existing launcher of the SAME TYPE, regardless of name.
        let same_type_existing: Vec<String> = ctx
            .config
            .launchers
            .values()
            .filter(|lc| lc.launcher_type == launcher_type && lc.launcher_id != instance_id)
            .map(|lc| lc.launcher_id.clone())
            .collect();

        // If the user wants to update an existing same-type instance, redirect
        // `instance_id` to that entry so the normal overwrite path fires.
        let instance_id = if !same_type_existing.is_empty() {
            ctx.ui.info(&format!(
                "\nNote: a launcher of type '{}' already exists: {}",
                launcher_type,
                same_type_existing.join(", ")
            ));
            let update_existing = ctx.ui.confirm(
                &format!(
                    "Update '{}' instead of creating '{}'?",
                    same_type_existing[0], instance_id
                ),
                false,
            )?;
            if update_existing {
                same_type_existing[0].clone()
            } else {
                instance_id
            }
        } else {
            instance_id
        };

        // Standard same-id overwrite check
        if ctx.config.get_launcher(&instance_id).is_some() {
            let overwrite = ctx.ui.confirm(
                &format!("Launcher '{instance_id}' is already configured. Overwrite?"),
                false,
            )?;
            if !overwrite {
                ctx.ui.info("Launcher setup skipped.");
                return Ok(());
            }
        }

        // Prompt for type-specific config via schema.
        // Existing config (for overwrites) takes precedence over registry defaults.
        let schema = LAUNCHER_REGISTRY
            .config_schema(launcher_type)
            .ok_or_else(|| {
                anyhow::anyhow!("No config schema registered for launcher type '{launcher_type}'")
            })?;
        let defaults = ctx
            .config
            .get_launcher(&instance_id)
            .map(|lc| lc.config.clone())
            .or_else(|| LAUNCHER_REGISTRY.default_config(launcher_type))
            .unwrap_or_else(|| serde_json::json!({}));

        let mut config = prompt_from_schema(&*ctx.ui, &schema, &defaults)?;

        // Normalise: an empty string for command_path means "use PATH" — treat
        // it the same as absent so validate_command does a PATH lookup.
        if config.get("command_path").and_then(|v| v.as_str()) == Some("") {
            config
                .as_object_mut()
                .map(|m| m.insert("command_path".to_string(), serde_json::Value::Null));
        }

        // Validate the binary now so the user gets immediate feedback.
        // validate_command respects command_path when set; falls back to PATH.
        let launcher = LAUNCHER_REGISTRY
            .construct(launcher_type, &config)
            .map_err(|e| anyhow::anyhow!("Failed to construct launcher: {e}"))?;

        match launcher.validate_command() {
            Ok(path) => {
                ctx.ui.info(&format!("  Binary found: {}", path.display()));
            }
            Err(e) => {
                // command_path was explicitly set but invalid, or binary not on PATH.
                anyhow::bail!(
                    "Binary validation failed: {e}\n\
                     Set command_path to the full path of the binary and re-run setup."
                );
            }
        }

        let launcher_config = crate::config::LauncherConfig {
            launcher_id: instance_id.clone(),
            launcher_type: launcher_type.to_string(),
            enabled_capabilities: vec![],
            config,
        };

        if let Err(e) = ctx.config.insert_launcher(&instance_id, launcher_config) {
            ctx.ui.warn(&format!("Failed to save launcher config: {e}"));
        }

        ctx.ui.info(&format!(
            "\nLauncher '{instance_id}' configured successfully!"
        ));
        if !launcher_def.supported_capabilities.is_empty() {
            ctx.ui.info("Supported capabilities:");
            for cap in &launcher_def.supported_capabilities {
                ctx.ui.info(&format!("  - {cap}"));
            }
        }

        Ok(())
    }

    /// Remove a configured launcher instance by ID.
    ///
    /// Deletes the launcher's config file and removes it from the in-memory
    /// config. After this call `launcher list` will no longer show the entry
    /// and `granite-cli launch <id>` will return an error.
    pub fn remove(ctx: &mut crate::AppContext, launcher_id: &str) -> Result<()> {
        if ctx.config.get_launcher(launcher_id).is_none() {
            anyhow::bail!("No launcher configured with id '{launcher_id}'. Nothing to remove.");
        }

        if let Err(e) = ctx.config.remove_launcher(launcher_id) {
            ctx.ui
                .warn(&format!("failed to persist launcher removal: {e}"));
        }
        ctx.ui.info(&format!("Launcher '{launcher_id}' removed."));
        Ok(())
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, LauncherConfig};
    use crate::utils::ui::base::tests::CaptureUi;
    use std::sync::Arc;

    fn test_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Arc::new(CaptureUi::default()),
        }
    }

    fn ctx_with_launcher(id: &str, launcher_type: &str) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.launchers.insert(
            id.to_string(),
            LauncherConfig {
                launcher_id: id.to_string(),
                launcher_type: launcher_type.to_string(),
                ..LauncherConfig::default()
            },
        );
        ctx
    }

    macro_rules! tables {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .tables
                .borrow()
        };
    }

    macro_rules! infos {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .infos
                .borrow()
        };
    }

    // -- catalog ---------------------------------------------------------------

    #[test]
    fn catalog_has_id_and_default_command_columns() {
        let ctx = test_ctx();
        LauncherCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        assert_eq!(tables.len(), 1);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"DEFAULT COMMAND".to_string()));
    }

    #[test]
    fn catalog_contains_claude_and_bob() {
        let ctx = test_ctx();
        LauncherCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.iter().any(|r| r[0] == "claude"));
        assert!(rows.iter().any(|r| r[0] == "bob"));
    }

    // -- list ------------------------------------------------------------------

    #[test]
    fn list_empty_config_has_zero_rows() {
        let ctx = test_ctx();
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_configured_launcher_shows_path_sentinel() {
        let ctx = ctx_with_launcher("my-claude", "claude");
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        // No command_path set → should show "(PATH)"
        assert!(rows[0].iter().any(|c| c == "(PATH)"));
    }

    #[test]
    fn list_columns_are_id_type_command() {
        let ctx = ctx_with_launcher("my-claude", "claude");
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"TYPE".to_string()));
        assert!(!headers.contains(&"ENABLED".to_string()));
        assert!(headers.contains(&"COMMAND".to_string()));
    }

    #[test]
    fn list_sorted_by_type_then_id() {
        let mut ctx = test_ctx();
        for (id, t) in [
            ("z-claude", "claude"),
            ("a-claude", "claude"),
            ("my-bob", "bob"),
        ] {
            ctx.config.launchers.insert(
                id.to_string(),
                LauncherConfig {
                    launcher_id: id.to_string(),
                    launcher_type: t.to_string(),
                    ..LauncherConfig::default()
                },
            );
        }
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows[0][1], "bob");
        assert_eq!(rows[1][0], "a-claude");
        assert_eq!(rows[2][0], "z-claude");
    }

    // -- setup (type-aware clash detection) ------------------------------------

    #[tokio::test]
    async fn setup_warns_on_same_type_existing_instance() {
        // Pre-populate a "claude" instance named "claude-old"
        let mut ctx = ctx_with_launcher("claude-old", "claude");
        // CaptureUi confirm always returns false → user declines update and
        // proceeds with the new name. The wizard then fails at binary
        // validation (claude not on PATH in CI), but by that point the clash
        // info message must already have been emitted.
        let _ = LauncherCommands::setup(&mut ctx, "claude", Some("claude-new")).await;
        let infos = infos!(ctx);
        assert!(
            infos.iter().any(|m| m.contains("claude-old")),
            "expected clash warning to mention the existing instance"
        );
    }

    #[tokio::test]
    async fn setup_unknown_type_returns_err() {
        let mut ctx = test_ctx();
        let result = LauncherCommands::setup(&mut ctx, "no-such-type", Some("test")).await;
        assert!(result.is_err());
    }

    // -- remove ----------------------------------------------------------------

    #[test]
    fn remove_existing_launcher_succeeds_and_disappears_from_list() {
        let mut ctx = ctx_with_launcher("my-claude", "claude");
        assert!(ctx.config.get_launcher("my-claude").is_some());

        LauncherCommands::remove(&mut ctx, "my-claude").unwrap();

        assert!(ctx.config.get_launcher("my-claude").is_none());
        let infos = infos!(ctx);
        assert!(
            infos
                .iter()
                .any(|m| m.contains("my-claude") && m.contains("removed"))
        );
    }

    #[test]
    fn remove_nonexistent_launcher_returns_err() {
        let mut ctx = test_ctx();
        let result = LauncherCommands::remove(&mut ctx, "doesnt-exist");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Nothing to remove")
        );
    }

    #[test]
    fn list_does_not_show_removed_launcher() {
        let mut ctx = ctx_with_launcher("my-claude", "claude");
        LauncherCommands::remove(&mut ctx, "my-claude").unwrap();
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.is_empty());
    }
}
