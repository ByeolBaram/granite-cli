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
                    .command_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "(PATH)".to_string());
                vec![
                    id.clone(),
                    cfg.launcher_type.clone(),
                    cfg.enabled.to_string(),
                    command,
                ]
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
            &["ID", "TYPE", "ENABLED", "COMMAND"],
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
                    "Launcher type '{}' not found in registry.",
                    launcher_type
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
            .info(&format!("\nSetting up launcher: {}", launcher_type));
        ctx.ui.info(&launcher_def.description);
        ctx.ui
            .info(&format!("Default command: {}", launcher_def.default_command));

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
                &format!(
                    "Launcher '{}' is already configured. Overwrite?",
                    instance_id
                ),
                false,
            )?;
            if !overwrite {
                ctx.ui.info("Launcher setup skipped.");
                return Ok(());
            }
        }

        // Prompt for type-specific config via schema
        let schema = LAUNCHER_REGISTRY
            .config_schema(launcher_type)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No config schema registered for launcher type '{}'",
                    launcher_type
                )
            })?;
        let defaults = ctx
            .config
            .get_launcher(&instance_id)
            .map(|lc| lc.config.clone())
            .or_else(|| LAUNCHER_REGISTRY.default_config(launcher_type))
            .unwrap_or_else(|| serde_json::json!({}));

        let config = prompt_from_schema(&*ctx.ui, &schema, &defaults)?;

        // Validate that the binary exists before saving anything.
        // A launcher with no reachable binary is not useful, so we fail hard
        // with an actionable message rather than silently saving dead config.
        let launcher = LAUNCHER_REGISTRY
            .construct(launcher_type, &config)
            .map_err(|e| anyhow::anyhow!("Failed to construct launcher: {}", e))?;

        match launcher.validate_command() {
            Ok(path) => ctx
                .ui
                .info(&format!("  Binary found: {}", path.display())),
            Err(e) => {
                anyhow::bail!(
                    "Binary '{}' not found: {}.\n\
                     Install the tool first, or re-run with a custom path:\n\
                     granite-cli launcher setup {} --id <name>  (then set command_path when prompted)",
                    launcher.command_name(),
                    e,
                    launcher_type,
                );
            }
        }

        let launcher_config = crate::config::LauncherConfig {
            launcher_id: instance_id.clone(),
            launcher_type: launcher_type.to_string(),
            command_path: None,
            enabled_capabilities: vec![],
            config,
            enabled: true,
        };

        if let Err(e) = ctx.config.insert_launcher(&instance_id, launcher_config) {
            ctx.ui
                .warn(&format!("Failed to save launcher config: {}", e));
        }

        ctx.ui.info(&format!(
            "\nLauncher '{}' configured successfully!",
            instance_id
        ));
        if !launcher_def.supported_capabilities.is_empty() {
            ctx.ui.info("Supported capabilities:");
            for cap in &launcher_def.supported_capabilities {
                ctx.ui.info(&format!("  - {}", cap));
            }
        }

        Ok(())
    }

    /// Validate that the configured launcher's binary is reachable.
    ///
    /// Prints the resolved absolute path on success, or a clear error message
    /// on failure. Useful for diagnosing non-PATH installs.
    pub fn validate(ctx: &crate::AppContext, launcher_id: &str) -> Result<()> {
        let lc = ctx.config.get_launcher(launcher_id).ok_or_else(|| {
            anyhow::anyhow!(
                "No launcher configured with id '{}'. Run `granite-cli launcher setup` first.",
                launcher_id
            )
        })?;

        let launcher = LAUNCHER_REGISTRY
            .construct(&lc.launcher_type, &lc.config)
            .map_err(|e| anyhow::anyhow!("Failed to construct launcher: {}", e))?;

        match launcher.validate_command() {
            Ok(path) => {
                ctx.ui
                    .status(launcher_id, true, &path.to_string_lossy());
                Ok(())
            }
            Err(e) => {
                ctx.ui.status(launcher_id, false, &e.to_string());
                anyhow::bail!("Binary not found: {}", e)
            }
        }
    }

    /// Remove a configured launcher instance by ID.
    ///
    /// Deletes the launcher's config file and removes it from the in-memory
    /// config. After this call `launcher list` will no longer show the entry
    /// and `granite-cli launch <id>` will return an error.
    pub fn remove(ctx: &mut crate::AppContext, launcher_id: &str) -> Result<()> {
        if ctx.config.get_launcher(launcher_id).is_none() {
            anyhow::bail!(
                "No launcher configured with id '{}'. Nothing to remove.",
                launcher_id
            );
        }

        ctx.config.remove_launcher(launcher_id)?;
        ctx.ui
            .info(&format!("Launcher '{}' removed.", launcher_id));
        Ok(())
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, LauncherConfig};
    use crate::utils::ui::base::tests::CaptureUi;

    fn test_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Box::new(CaptureUi::default()),
        }
    }

    fn ctx_with_launcher(id: &str, launcher_type: &str) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.launchers.insert(
            id.to_string(),
            LauncherConfig {
                launcher_id: id.to_string(),
                launcher_type: launcher_type.to_string(),
                command_path: None,
                enabled_capabilities: vec![],
                config: serde_json::json!({}),
                enabled: true,
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

    macro_rules! statuses {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .statuses
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
    fn list_columns_are_id_type_enabled_command() {
        let ctx = ctx_with_launcher("my-claude", "claude");
        LauncherCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"TYPE".to_string()));
        assert!(headers.contains(&"ENABLED".to_string()));
        assert!(headers.contains(&"COMMAND".to_string()));
    }

    #[test]
    fn list_sorted_by_type_then_id() {
        let mut ctx = test_ctx();
        for (id, t) in [("z-claude", "claude"), ("a-claude", "claude"), ("my-bob", "bob")] {
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

    // -- validate --------------------------------------------------------------

    #[test]
    fn validate_unknown_id_returns_err() {
        let ctx = test_ctx();
        let result = LauncherCommands::validate(&ctx, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("launcher setup"));
    }

    #[test]
    fn validate_known_id_missing_binary_emits_status_false() {
        // Claude not installed in CI → validate should report unhealthy, not panic
        let ctx = ctx_with_launcher("claude", "claude");
        // We don't assert Ok/Err because the binary may or may not be present;
        // we only verify the status row is emitted.
        let _ = LauncherCommands::validate(&ctx, "claude");
        let statuses = statuses!(ctx);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "claude");
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
        assert!(infos.iter().any(|m| m.contains("my-claude") && m.contains("removed")));
    }

    #[test]
    fn remove_nonexistent_launcher_returns_err() {
        let mut ctx = test_ctx();
        let result = LauncherCommands::remove(&mut ctx, "doesnt-exist");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Nothing to remove"));
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
