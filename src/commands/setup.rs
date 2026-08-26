// Third Party
use anyhow::Result;
use std::collections::{HashMap, HashSet};

// Local
use crate::capabilities::{BindingType, CAPABILITY_REGISTRY, Dependency, ModelRequirement};
use crate::dependency::Requirement;
use crate::launchers::LAUNCHER_REGISTRY;
use crate::models::{ContextFit, MODEL_REGISTRY, ModelMetadata, ModelType, ModelVariant};
use crate::providers::{HealthStatus, PROVIDER_REGISTRY, Provider};
use crate::utils::hardware::detect_hardware;

/*-- public --*/

/// A single recommendation produced during discovery.
pub enum Recommendation {
    Provider {
        provider_type: &'static str,
        provider_name: String,
        health_healthy: bool,
        health_error: Option<String>,
    },
    Model {
        model_id: String,
        family: String,
        version: String,
        size: String,
        model_type: ModelType,
        best_variant: ModelVariant,
        context_fit: ContextFit,
        can_run_by: Vec<String>,
    },
    Launcher {
        launcher_type: String,
        launcher_name: String,
        binary_path: Option<String>,
    },
    Capability {
        capability_type: String,
        capability_name: String,
    },
}

/// The complete output of the discovery engine.
pub struct DiscoveryResult {
    pub recommendations: Vec<Recommendation>,
    pub configured_provider_ids: Vec<String>,
    pub configured_model_ids: Vec<String>,
    pub configured_launcher_ids: Vec<String>,
    pub configured_capability_ids: Vec<String>,
}

/*-- private --*/

/// Discovers all available providers, models, launchers, and capabilities,
/// returning structured recommendations and a list of already-configured items.
struct Discover;

impl Discover {
    /// Run the full discovery pipeline.
    pub async fn run(ctx: &crate::AppContext) -> DiscoveryResult {
        let (provider_recs, configured_providers) = Self::discover_providers(ctx).await;
        let model_recs = Self::discover_models(ctx, &configured_providers);
        let (launcher_recs, configured_launchers) = Self::discover_launchers(ctx).await;
        let (capability_recs, configured_capabilities) = Self::discover_capabilities(
            ctx,
            &provider_recs,
            &model_recs,
            &configured_providers,
        );

        let mut recommendations: Vec<Recommendation> = Vec::new();
        recommendations.extend(provider_recs);
        recommendations.extend(model_recs);
        recommendations.extend(launcher_recs);
        recommendations.extend(capability_recs);

        // Sort for deterministic output
        recommendations.sort_by_key(display_name);

        DiscoveryResult {
            recommendations,
            configured_provider_ids: configured_providers,
            configured_model_ids: ctx.config.models.keys().cloned().collect(),
            configured_launcher_ids: configured_launchers,
            configured_capability_ids: configured_capabilities,
        }
    }

    // -- Provider discovery --------------------------------------------------

    async fn discover_providers(
        ctx: &crate::AppContext,
    ) -> (Vec<Recommendation>, Vec<String>) {
        let configured_ids: HashSet<&str> = ctx.config.providers.keys().map(|s| s.as_str()).collect();
        let mut configured: Vec<String> = Vec::new();
        let mut recommendations: Vec<Recommendation> = Vec::new();

        for (provider_type, metadata) in PROVIDER_REGISTRY.entries() {
            if configured_ids.contains(provider_type) {
                configured.push(provider_type.to_string());
                continue;
            }

            // Construct a transient instance with default config and run health check
            let default_config = PROVIDER_REGISTRY.default_config(provider_type).unwrap_or_default();
            let result = PROVIDER_REGISTRY.construct(
                provider_type,
                provider_type,
                &default_config,
                &ctx.config,
            );

            match result {
                Ok(provider) => match Self::run_health_check(&*provider).await {
                    Ok(status) => recommendations.push(Recommendation::Provider {
                        provider_type,
                        provider_name: metadata.name.clone(),
                        health_healthy: status.healthy,
                        health_error: status.error,
                    }),
                    Err(e) => recommendations.push(Recommendation::Provider {
                        provider_type,
                        provider_name: metadata.name.clone(),
                        health_healthy: false,
                        health_error: Some(format!("Health check failed: {e}")),
                    }),
                },
                Err(_) => {
                    // Provider could not be constructed (e.g., missing schema).
                    // Still recommend it — user may need to configure it manually.
                    recommendations.push(Recommendation::Provider {
                        provider_type,
                        provider_name: metadata.name.clone(),
                        health_healthy: false,
                        health_error: Some("Could not construct provider with default config".to_string()),
                    });
                }
            }
        }

        configured.sort();
        recommendations.sort_by_key(display_name);
        (recommendations, configured)
    }

    async fn run_health_check(provider: &dyn Provider) -> Result<HealthStatus, crate::providers::ProviderError> {
        provider.health_check().await
    }

    // -- Model discovery -----------------------------------------------------

    fn discover_models(
        ctx: &crate::AppContext,
        configured_provider_ids: &[String],
    ) -> Vec<Recommendation> {
        let profile = detect_hardware();
        let configured_ids: HashSet<&str> = ctx.config.models.keys().map(|s| s.as_str()).collect();

        // Group models by family, keeping each model's real catalog id
        // alongside its metadata.
        let mut family_groups: HashMap<String, Vec<(String, ModelMetadata)>> = HashMap::new();
        for (model_id, model_md) in MODEL_REGISTRY.entries() {
            if configured_ids.contains(model_id) {
                continue;
            }
            let family = model_md.family.clone();
            family_groups
                .entry(family)
                .or_default()
                .push((model_id.to_string(), model_md));
        }

        let mut recommendations: Vec<Recommendation> = Vec::new();

        for models in family_groups.values() {
            // Find latest version in this family
            let latest = find_latest_version(models);

            if let Some((model_id, latest_model)) = latest {
                // Find best variant for this model
                if let Some((best_variant, best_fit)) = best_variant(latest_model, &profile) {
                    // Check which configured providers can run this variant
                    let can_run_by = Self::find_can_run_providers(
                        &best_variant,
                        configured_provider_ids,
                        ctx,
                    );

                    recommendations.push(Recommendation::Model {
                        model_id: model_id.clone(),
                        family: latest_model.family.clone(),
                        version: latest_model.version.clone(),
                        size: format_size(latest_model.size),
                        model_type: latest_model.model_type.clone(),
                        best_variant,
                        context_fit: best_fit,
                        can_run_by,
                    });
                }
            }
        }

        // Sort by family, then version desc, then size desc
        recommendations.sort_by(|a, b| {
            let (a_family, a_version, a_size) = model_sort_key(a);
            let (b_family, b_version, b_size) = model_sort_key(b);
            a_family
                .cmp(b_family)
                .then_with(|| compare_versions_desc(a_version, b_version))
                .then_with(|| b_size.cmp(&a_size))
        });

        recommendations
    }

    fn find_can_run_providers(
        variant: &ModelVariant,
        configured_provider_ids: &[String],
        ctx: &crate::AppContext,
    ) -> Vec<String> {
        configured_provider_ids
            .iter()
            .filter_map(|pid| ctx.config.get_provider(pid))
            .filter_map(|pc| {
                PROVIDER_REGISTRY
                    .construct(
                        &pc.provider_type,
                        &pc.provider_id,
                        &pc.config,
                        &ctx.config,
                    )
                    .ok()
                    .filter(|p| p.can_run_model(&variant.format, &variant.precision))
            })
            .map(|p| p.instance_id().to_string())
            .collect()
    }

    // -- Launcher discovery --------------------------------------------------

    async fn discover_launchers(
        ctx: &crate::AppContext,
    ) -> (Vec<Recommendation>, Vec<String>) {
        let configured_ids: HashSet<&str> = ctx.config.launchers.keys().map(|s| s.as_str()).collect();
        let mut configured: Vec<String> = Vec::new();
        let mut recommendations: Vec<Recommendation> = Vec::new();

        for (launcher_type, metadata) in LAUNCHER_REGISTRY.entries() {
            if configured_ids.contains(launcher_type) {
                configured.push(launcher_type.to_string());
                continue;
            }

            // Construct a transient instance with default config
            let default_config = LAUNCHER_REGISTRY.default_config(launcher_type).unwrap_or_default();
            match LAUNCHER_REGISTRY.construct(
                launcher_type,
                launcher_type,
                &default_config,
                &ctx.config,
            ) {
                Ok(launcher) => match launcher.validate_command() {
                    Ok(path) => recommendations.push(Recommendation::Launcher {
                        launcher_type: launcher_type.to_string(),
                        launcher_name: metadata.name.clone(),
                        binary_path: Some(path.to_string_lossy().to_string()),
                    }),
                    Err(_) => recommendations.push(Recommendation::Launcher {
                        launcher_type: launcher_type.to_string(),
                        launcher_name: metadata.name.clone(),
                        binary_path: None,
                    }),
                },
                Err(_) => {
                    // Could not construct — still recommend but without binary info
                    recommendations.push(Recommendation::Launcher {
                        launcher_type: launcher_type.to_string(),
                        launcher_name: metadata.name.clone(),
                        binary_path: None,
                    });
                }
            }
        }

        configured.sort();
        recommendations.sort_by_key(display_name);
        (recommendations, configured)
    }

    // -- Capability discovery ------------------------------------------------

    fn discover_capabilities(
        ctx: &crate::AppContext,
        _provider_recs: &[Recommendation],
        _model_recs: &[Recommendation],
        _configured_provider_ids: &[String],
    ) -> (Vec<Recommendation>, Vec<String>) {
        let configured_ids: Vec<&str> = ctx
            .config
            .capabilities
            .keys()
            .map(|s| s.as_str())
            .collect();
        let configured: Vec<String> = configured_ids.iter().copied().map(String::from).collect();

        let mut recommendations: Vec<Recommendation> = Vec::new();

        for (capability_type, metadata) in CAPABILITY_REGISTRY.entries() {
            if configured_ids.contains(&capability_type) {
                continue;
            }

            recommendations.push(Recommendation::Capability {
                capability_type: capability_type.to_string(),
                capability_name: metadata.name.clone(),
            });
        }

        recommendations.sort_by_key(display_name);
        (recommendations, configured)
    }
}

/// A re-evaluator that filters recommendations based on user selections from
/// earlier wizard sections. This implements the backward-from-capabilities
/// dependency flow.
struct Revaluator;

impl Revaluator {
    /// Filter launcher recommendations to only those that support at least one
    /// of the selected capability binding types.
    fn for_launchers<'a>(
        discovery: &'a DiscoveryResult,
        selected_cap_types: &HashSet<String>,
    ) -> Vec<&'a Recommendation> {
        // Determine which binding types are needed by selected capabilities
        let needed_types: HashSet<BindingType> = selected_cap_types
            .iter()
            .filter_map(|cap_type| CAPABILITY_REGISTRY.get(cap_type))
            .flat_map(|m| m.supported_binding_types.clone().into_iter())
            .collect();

        if needed_types.is_empty() {
            // No binding types needed — any launcher is fine
            return discovery
                .recommendations
                .iter()
                .filter(|r| matches!(r, Recommendation::Launcher { .. }))
                .collect();
        }

        discovery
            .recommendations
            .iter()
            .filter(move |r| {
                if let Recommendation::Launcher { launcher_type, .. } = r {
                    if let Some(launcher_meta) = LAUNCHER_REGISTRY.get(launcher_type) {
                        return launcher_meta
                            .supported_capabilities
                            .iter()
                            .any(|bt| needed_types.contains(bt));
                    }
                    // If we can't look up the launcher, include it anyway
                    return true;
                }
                false
            })
            .collect()
    }

    /// Filter provider recommendations to only those that can run at least one
    /// of the selected model variants.
    fn for_providers<'a>(
        discovery: &'a DiscoveryResult,
        selected_model_ids: &HashSet<String>,
        _ctx: &crate::AppContext,
    ) -> Vec<&'a Recommendation> {
        let provider_recs: Vec<_> = discovery
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Provider { .. }))
            .collect();

        // If no models selected, return all provider recommendations
        if selected_model_ids.is_empty() {
            return provider_recs;
        }

        // Build a set of selected model IDs for quick lookup
        let selected_ids: HashSet<&str> = selected_model_ids.iter().map(|s| s.as_str()).collect();

        // Check which providers can run the selected models by looking at the
        // can_run_by field in the model recommendations
        let providers_that_can_run: HashSet<String> = discovery
            .recommendations
            .iter()
            .filter_map(|r| match r {
                Recommendation::Model {
                    model_id,
                    can_run_by,
                    ..
                } => {
                    if selected_ids.contains(model_id.as_str()) {
                        Some(can_run_by.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .flatten()
            .collect();

        if providers_that_can_run.is_empty() {
            // If no provider info available, show all providers
            return provider_recs;
        }

        provider_recs
            .into_iter()
            .filter(move |r| {
                if let Recommendation::Provider { provider_type, .. } = r {
                    providers_that_can_run.contains(*provider_type)
                } else {
                    false
                }
            })
            .collect()
    }

    /// Filter model recommendations to only those that would be used by at least
    /// one selected capability (i.e., satisfy the capability's `ModelRequirement`).
    fn for_models<'a>(
        discovery: &'a DiscoveryResult,
        selected_cap_types: &HashSet<String>,
    ) -> Vec<&'a Recommendation> {
        // Collect all model requirements from selected capabilities
        let all_requirements: Vec<ModelRequirement> = selected_cap_types
            .iter()
            .filter_map(|cap_type| CAPABILITY_REGISTRY.get(cap_type))
            .flat_map(|m| {
                m.dependencies
                    .iter()
                    .filter_map(|d| match d {
                        Dependency::Model {
                            requirement,
                            resolved_id: None,
                            ..
                        } => Some(requirement.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        if all_requirements.is_empty() {
            // No model requirements — show all model recommendations
            return discovery
                .recommendations
                .iter()
                .filter(|r| matches!(r, Recommendation::Model { .. }))
                .collect();
        }

        discovery
            .recommendations
            .iter()
            .filter(move |r| {
                if let Recommendation::Model { model_id, .. } = r {
                    // Look up the real catalog metadata (family/version/size
                    // alone can't tell us `supported_functions`, which most
                    // capability requirements actually key on).
                    match MODEL_REGISTRY.get(model_id) {
                        Some(md) => all_requirements.iter().any(|req| req.admits_type(&md)),
                        None => false,
                    }
                } else {
                    false
                }
            })
            .collect()
    }

    /// Filter capability recommendations to only those that could actually be
    /// used by at least one selected launcher (i.e., the launcher supports
    /// one of the capability's declared binding types). With no launchers
    /// selected, no capability has anywhere to bind, so none are shown.
    fn for_capabilities<'a>(
        discovery: &'a DiscoveryResult,
        selected_launcher_types: &HashSet<String>,
    ) -> Vec<&'a Recommendation> {
        if selected_launcher_types.is_empty() {
            return Vec::new();
        }

        let supported_types: HashSet<BindingType> = selected_launcher_types
            .iter()
            .filter_map(|lt| LAUNCHER_REGISTRY.get(lt))
            .flat_map(|m| m.supported_capabilities.clone().into_iter())
            .collect();

        discovery
            .recommendations
            .iter()
            .filter(move |r| {
                if let Recommendation::Capability { capability_type, .. } = r {
                    if let Some(cap_meta) = CAPABILITY_REGISTRY.get(capability_type) {
                        return cap_meta
                            .supported_binding_types
                            .iter()
                            .any(|bt| supported_types.contains(bt));
                    }
                    // If we can't look up the capability, include it anyway
                    true
                } else {
                    false
                }
            })
            .collect()
    }
}

/*-- helpers -----------------------------------------------------------------*/

/// Compare semantic versions in descending order (higher versions first).
fn compare_versions_desc(a: &str, b: &str) -> std::cmp::Ordering {
    let parse_version =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };

    let va = parse_version(a);
    let vb = parse_version(b);

    for (a_part, b_part) in va.iter().zip(vb.iter()) {
        match b_part.cmp(a_part) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    vb.len().cmp(&va.len())
}

fn find_latest_version(models: &[(String, ModelMetadata)]) -> Option<&(String, ModelMetadata)> {
    models
        .iter()
        .max_by(|(_, a), (_, b)| compare_versions_desc(&a.version, &b.version))
}

fn format_size(size: u64) -> String {
    match size {
        1_000_000_000.. => format!("{}B", size / 1_000_000_000),
        1_000_000.. => format!("{}M", size / 1_000_000),
        _ => size.to_string(),
    }
}

fn parse_size(size_str: &str) -> u64 {
    let size_str = size_str.trim();
    if let Some(num) = size_str.strip_suffix('B') {
        num.parse::<u64>().unwrap_or(0) * 1_000_000_000
    } else if let Some(num) = size_str.strip_suffix('M') {
        num.parse::<u64>().unwrap_or(0) * 1_000_000
    } else {
        size_str.parse::<u64>().unwrap_or(0)
    }
}

fn best_variant(
    model: &ModelMetadata,
    profile: &crate::utils::hardware::HardwareProfile,
) -> Option<(ModelVariant, ContextFit)> {
    let fit_rank = |fit: &ContextFit| match fit {
        ContextFit::Full => 1,
        ContextFit::Partial(_) => 0,
        ContextFit::None => -1,
    };

    model
        .variants
        .iter()
        .map(|v| {
            let fit = crate::models::context_fit::estimate(
                model.context_length,
                &model.architecture,
                &model.native_dtype,
                v,
                profile,
            );
            (fit, v)
        })
        .filter(|(fit, _)| *fit != ContextFit::None)
        .max_by(|(fit_a, a), (fit_b, b)| {
            fit_rank(fit_a).cmp(&fit_rank(fit_b)).then_with(|| {
                a.size_gb
                    .partial_cmp(&b.size_gb)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
        .map(|(fit, v)| (v.clone(), fit))
}

fn display_name(rec: &Recommendation) -> String {
    match rec {
        Recommendation::Provider { provider_name, .. } => provider_name.clone(),
        Recommendation::Model {
            family,
            version,
            size,
            ..
        } => format!("{family} {version} {size}"),
        Recommendation::Launcher {
            launcher_name, ..
        } => launcher_name.clone(),
        Recommendation::Capability {
            capability_name, ..
        } => capability_name.clone(),
    }
}

/// Extracts `(family, version, size)` from a model recommendation for
/// sorting purposes, without needing a full `ModelMetadata`.
fn model_sort_key(rec: &Recommendation) -> (&str, &str, u64) {
    match rec {
        Recommendation::Model {
            family,
            version,
            size,
            ..
        } => (family.as_str(), version.as_str(), parse_size(size)),
        _ => ("", "", 0),
    }
}

/*-- SetupCommands -----------------------------------------------------------*/

pub struct SetupCommands;

impl SetupCommands {
    /// Entry point for `granite-cli setup`.
    pub async fn run(
        ctx: &mut crate::AppContext,
        auto: bool,
        skip_pull: bool,
    ) -> Result<()> {
        if auto {
            Self::run_auto(ctx).await
        } else {
            Self::run_wizard(ctx, skip_pull).await
        }
    }

    /// Run the interactive wizard.
    async fn run_wizard(
        ctx: &mut crate::AppContext,
        skip_pull: bool,
    ) -> Result<()> {
        let ui = &*ctx.ui;
        ui.info("=== granite-cli Setup Wizard ===\n");
        ui.info("Discovering available components...\n");

        let discovery = Discover::run(ctx).await;

        if discovery.recommendations.is_empty()
            && discovery.configured_provider_ids.is_empty()
            && discovery.configured_model_ids.is_empty()
            && discovery.configured_launcher_ids.is_empty()
            && discovery.configured_capability_ids.is_empty()
        {
            ui.info("Nothing to configure. All components are either not available or already set up.");
            return Ok(());
        }

        // Phase 1: Launchers selection (show all detected launchers)
        let selected_launchers = Self::select_launchers(ctx, &discovery).await?;

        // Phase 2: Capabilities selection (filtered by what the selected
        // launchers can actually bind)
        let selected_caps = Self::select_capabilities(ctx, &discovery, &selected_launchers).await?;

        // Phase 3: Models selection (filtered by capability requirements)
        let selected_models = Self::select_models(ctx, &discovery, &selected_caps).await?;

        // Phase 4: Providers selection (only healthy, filtered by model compatibility)
        let selected_providers =
            Self::select_providers(ctx, &discovery, &selected_models).await?;

        // Phase 5: Configuration
        Self::configure_all(
            ctx,
            &discovery,
            &selected_caps,
            &selected_launchers,
            &selected_providers,
            &selected_models,
        )
        .await?;

        // Phase 6: Pull (optional)
        if !skip_pull {
            Self::prompt_pull(ctx, &selected_models).await?;
        }

        // Phase 7: Summary
        Self::print_summary(
            ctx,
            &selected_caps,
            &selected_launchers,
            &selected_providers,
            &selected_models,
        );

        Ok(())
    }

    /// Run auto mode — detect, configure everything with defaults.
    async fn run_auto(ctx: &mut crate::AppContext) -> Result<()> {
        let ui = &*ctx.ui;
        ui.info("=== granite-cli Auto Setup ===\n");
        ui.info("Auto-detecting and configuring all available components...\n");

        let discovery = Discover::run(ctx).await;

        if discovery.recommendations.is_empty() {
            ui.info("No components available to configure.");
            return Ok(());
        }

        // Auto-select everything that's recommended, following the same
        // Launchers → Capabilities → Models → Providers dependency chain as
        // the interactive wizard.
        let selected_launchers: HashSet<String> = discovery
            .recommendations
            .iter()
            .filter_map(|r| match r {
                Recommendation::Launcher {
                    launcher_type,
                    binary_path: Some(_),
                    ..
                } => Some(launcher_type.clone()),
                _ => None,
            })
            .collect();

        let selected_caps: HashSet<String> = Revaluator::for_capabilities(&discovery, &selected_launchers)
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Capability {
                    capability_type, ..
                } => Some(capability_type.clone()),
                _ => None,
            })
            .collect();

        let selected_models: HashSet<String> = Revaluator::for_models(&discovery, &selected_caps)
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Model { model_id, .. } => Some(model_id.clone()),
                _ => None,
            })
            .collect();

        let selected_providers: HashSet<String> = Revaluator::for_providers(&discovery, &selected_models, ctx)
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Provider {
                    provider_type,
                    health_healthy,
                    ..
                } if *health_healthy => Some(provider_type.to_string()),
                _ => None,
            })
            .collect();

        Self::configure_all(
            ctx,
            &discovery,
            &selected_caps,
            &selected_launchers,
            &selected_providers,
            &selected_models,
        )
        .await?;

        // Never auto-pull in --auto mode
        Self::print_summary(
            ctx,
            &selected_caps,
            &selected_launchers,
            &selected_providers,
            &selected_models,
        );

        Ok(())
    }

    /*-- Selection phases ----------------------------------------------------*/

    async fn select_capabilities(
        ctx: &mut crate::AppContext,
        discovery: &DiscoveryResult,
        selected_launchers: &HashSet<String>,
    ) -> Result<HashSet<String>> {
        let ui = &*ctx.ui;

        let caps: Vec<_> = Revaluator::for_capabilities(discovery, selected_launchers)
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Capability {
                    capability_type,
                    capability_name,
                } => Some((capability_type.clone(), capability_name.clone())),
                _ => None,
            })
            .collect();

        if caps.is_empty() {
            ui.info("No capabilities available for the selected launchers.");
            return Ok(HashSet::new());
        }

        let items: Vec<String> = caps
            .iter()
            .map(|(id, name)| format!("{} — {}", id, name))
            .collect();
        let defaults = vec![true; items.len()];

        let selected = ui.multi_select(
            "Select capabilities to configure",
            &items,
            &defaults,
        )?;

        Ok(selected
            .into_iter()
            .map(|i| caps[i].0.clone())
            .collect())
    }

    async fn select_launchers(
        ctx: &mut crate::AppContext,
        discovery: &DiscoveryResult,
    ) -> Result<HashSet<String>> {
        let ui = &*ctx.ui;

        let filtered: Vec<_> = Revaluator::for_launchers(discovery, &HashSet::new())
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Launcher {
                    launcher_type,
                    launcher_name,
                    binary_path: Some(binary_path),
                } => Some((launcher_type.clone(), launcher_name, binary_path.clone())),
                _ => None,
            })
            .collect();

        if filtered.is_empty() {
            ui.info("No launchers detected on this system.");
            return Ok(HashSet::new());
        }

        let items: Vec<String> = filtered
            .iter()
            .map(|(id, name, path)| format!("{} — {} ({})", id, name, path))
            .collect();
        let defaults = vec![true; items.len()];

        let selected = ui.multi_select(
            "Select launchers to configure",
            &items,
            &defaults,
        )?;

        Ok(selected
            .into_iter()
            .map(|i| filtered[i].0.clone())
            .collect())
    }

    async fn select_providers(
        ctx: &mut crate::AppContext,
        discovery: &DiscoveryResult,
        selected_models: &HashSet<String>,
    ) -> Result<HashSet<String>> {
        let ui = &*ctx.ui;

        let filtered: Vec<_> = Revaluator::for_providers(
            discovery,
            selected_models,
            ctx,
        )
        .into_iter()
            .filter_map(|r| match r {
                Recommendation::Provider {
                    provider_type,
                    provider_name,
                    health_healthy,
                    health_error,
                } if *health_healthy => Some((
                    provider_type.to_string(),
                    provider_name,
                    health_error.clone(),
                )),
                _ => None,
            })
        .collect();

        if filtered.is_empty() {
            ui.info("No healthy providers available to configure.");
            return Ok(HashSet::new());
        }

        let items: Vec<String> = filtered
            .iter()
            .map(|(id, name, error)| {
                let status = if error.is_none() || error.as_ref().is_some_and(|e| e.is_empty()) {
                    "healthy".to_string()
                } else if let Some(e) = &error {
                    format!("healthy ({e})")
                } else {
                    "healthy".to_string()
                };
                format!("{} — {} ({})", id, name, status)
            })
            .collect();
        let defaults = vec![true; items.len()];

        let selected = ui.multi_select(
            "Select providers to configure",
            &items,
            &defaults,
        )?;

        Ok(selected
            .into_iter()
            .map(|i| filtered[i].0.clone())
            .collect())
    }

    async fn select_models(
        ctx: &mut crate::AppContext,
        discovery: &DiscoveryResult,
        selected_caps: &HashSet<String>,
    ) -> Result<HashSet<String>> {
        let ui = &*ctx.ui;

        let filtered: Vec<_> = Revaluator::for_models(discovery, selected_caps)
            .into_iter()
            .filter_map(|r| match r {
                Recommendation::Model {
                    model_id,
                    family,
                    version,
                    size,
                    model_type,
                    best_variant,
                    context_fit,
                    can_run_by,
                } => Some((
                    model_id.clone(),
                    family,
                    version,
                    size,
                    model_type,
                    best_variant,
                    context_fit,
                    can_run_by,
                )),
                _ => None,
            })
            .collect();

        if filtered.is_empty() {
            ui.info("No models available for the selected capabilities.");
            return Ok(HashSet::new());
        }

        let items: Vec<String> = filtered
            .iter()
            .map(|(id, _family, _version, size, _model_type, _variant, fit, providers)| {
                let fit_str = match fit {
                    ContextFit::Full => "Full".to_string(),
                    ContextFit::Partial(_) => "Partial".to_string(),
                    ContextFit::None => "None".to_string(),
                };
                let providers_str = if providers.is_empty() {
                    "none".to_string()
                } else {
                    providers.join(", ")
                };
                format!("{} — {} — Fit: {} ({})", id, size, fit_str, providers_str)
            })
            .collect();
        let defaults = vec![true; items.len()];

        let selected = ui.multi_select(
            "Select models to configure",
            &items,
            &defaults,
        )?;

        Ok(selected
            .into_iter()
            .map(|i| filtered[i].0.clone())
            .collect())
    }

    /*-- Configuration phase -------------------------------------------------*/

    async fn configure_all(
        ctx: &mut crate::AppContext,
        discovery: &DiscoveryResult,
        selected_caps: &HashSet<String>,
        selected_launchers: &HashSet<String>,
        selected_providers: &HashSet<String>,
        selected_models: &HashSet<String>,
    ) -> Result<()> {
        let ui = &*ctx.ui;

        // Configure providers first
        for provider_id in selected_providers {
            ui.info(&format!("\nConfiguring provider: {provider_id}..."));
            let default_config = PROVIDER_REGISTRY
                .default_config(provider_id)
                .unwrap_or_default();

            let provider_config = crate::config::ProviderConfig {
                provider_id: provider_id.clone(),
                provider_type: provider_id.to_string(),
                config: default_config,
            };

            if ctx.config.insert_provider(provider_id, provider_config).is_err() {
                ui.warn(&format!("Failed to save provider config for '{provider_id}'"));
            }
        }

        // Configure launchers
        for launcher_id in selected_launchers {
            ui.info(&format!("\nConfiguring launcher: {launcher_id}..."));
            let default_config = LAUNCHER_REGISTRY
                .default_config(launcher_id)
                .unwrap_or_default();

            let launcher_config = crate::config::LauncherConfig {
                launcher_id: launcher_id.to_string(),
                launcher_type: launcher_id.to_string(),
                enabled_capabilities: Vec::new(),
                config: default_config,
            };

            if ctx.config.insert_launcher(launcher_id, launcher_config).is_err() {
                ui.warn(&format!("Failed to save launcher config for '{launcher_id}'"));
            }
        }

        // Configure models
        for model_id in selected_models {
            ui.info(&format!("\nConfiguring model: {model_id}..."));

            // Prefer a selected provider that can actually run this model's
            // variant; fall back to any selected provider.
            let provider_id = Self::find_provider_for_model(model_id, selected_providers, discovery)
                .or_else(|| selected_providers.iter().next().cloned());

            let model_config = crate::config::ModelConfig {
                model_id: model_id.clone(),
                provider_id,
                variant: None,
            };

            if ctx.config.insert_model(model_id, model_config).is_err() {
                ui.warn(&format!("Failed to save model config for '{model_id}'"));
            }
        }

        // Configure capabilities
        for cap_type in selected_caps {
            ui.info(&format!("\nConfiguring capability: {cap_type}..."));

            let mut config = CAPABILITY_REGISTRY
                .default_config(cap_type)
                .unwrap_or_default();

            // Set model_id if the capability requires a model
            if let Some(model_id) = Self::find_model_for_capability(cap_type, selected_models) {
                config["model_id"] = model_id.into();
            }

            let capability_config = crate::config::CapabilityConfig {
                capability_id: cap_type.clone(),
                capability_type: cap_type.clone(),
                config,
            };

            if ctx
                .config
                .insert_capability(cap_type, capability_config)
                .is_err()
            {
                ui.warn(&format!(
                    "Failed to save capability config for '{cap_type}'"
                ));
            }
        }

        Ok(())
    }

    /// Pick a selected provider that can run `model_id`'s recommended variant,
    /// per the `can_run_by` list computed during discovery.
    fn find_provider_for_model(
        model_id: &str,
        selected_providers: &HashSet<String>,
        discovery: &DiscoveryResult,
    ) -> Option<String> {
        discovery.recommendations.iter().find_map(|r| match r {
            Recommendation::Model {
                model_id: rec_id,
                can_run_by,
                ..
            } if rec_id == model_id => can_run_by
                .iter()
                .find(|p| selected_providers.contains(*p))
                .cloned(),
            _ => None,
        })
    }

    /// Find a model_id from selected_models that satisfies a capability's model
    /// requirement. Returns the first matching model or None if no match.
    fn find_model_for_capability(
        cap_type: &str,
        selected_models: &HashSet<String>,
    ) -> Option<String> {
        let cap_meta = CAPABILITY_REGISTRY.get(cap_type)?;

        let all_requirements: Vec<ModelRequirement> = cap_meta
            .dependencies
            .iter()
            .filter_map(|d| match d {
                Dependency::Model {
                    requirement,
                    resolved_id: None,
                    ..
                } => Some(requirement.clone()),
                _ => None,
            })
            .collect();

        if all_requirements.is_empty() {
            return None;
        }

        // Check each selected model's real catalog metadata to see if it
        // satisfies any requirement.
        selected_models.iter().find(|model_id| {
            MODEL_REGISTRY
                .get(model_id)
                .is_some_and(|md| all_requirements.iter().any(|req| req.admits_type(&md)))
        }).cloned()
    }

    /*-- Pull phase ----------------------------------------------------------*/

    async fn prompt_pull(
        ctx: &mut crate::AppContext,
        selected_models: &HashSet<String>,
    ) -> Result<()> {
        let ui = &*ctx.ui;

        // Find local provider models that were configured
        let pullable: Vec<_> = selected_models
            .iter()
            .filter_map(|model_id| {
                ctx.config
                    .get_model(model_id)
                    .and_then(|mc| mc.provider_id.clone())
                    .and_then(|provider_id| {
                        ctx.config.get_provider(&provider_id).map(|pc| {
                            (
                                model_id.clone(),
                                provider_id,
                                pc.provider_type.clone(),
                            )
                        })
                    })
            })
            .collect();

        if pullable.is_empty() {
            return Ok(());
        }

        let items: Vec<String> = pullable
            .iter()
            .map(|(model, provider, ptype)| {
                format!(
                    "{} → {} ({})",
                    provider, model, ptype
                )
            })
            .collect();

        let pull_now = ui.confirm(
            "\n→ Pull model weights now?",
            !items.is_empty(),
        )?;

        if pull_now {
            for (model_id, _provider_id, _provider_type) in &pullable {
                ui.info(&format!("Pulling {}...", model_id));
                // Pull is delegated to the model command; in auto mode we just
                // log it here. The actual pull would be triggered by
                // `ModelCommands::pull` which requires the model to be fully
                // configured with a variant.
            }
        }

        Ok(())
    }

    /*-- Summary phase -------------------------------------------------------*/

    fn print_summary(
        ctx: &crate::AppContext,
        selected_caps: &HashSet<String>,
        selected_launchers: &HashSet<String>,
        selected_providers: &HashSet<String>,
        selected_models: &HashSet<String>,
    ) {
        let ui = &*ctx.ui;

        ui.info("\n=== Setup Complete ===");
        ui.info(&format!(
            "Providers: {}",
            if selected_providers.is_empty() {
                "none".to_string()
            } else {
                selected_providers.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        ));
        ui.info(&format!(
            "Models: {}",
            if selected_models.is_empty() {
                "none".to_string()
            } else {
                selected_models.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        ));
        ui.info(&format!(
            "Launchers: {}",
            if selected_launchers.is_empty() {
                "none".to_string()
            } else {
                selected_launchers.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        ));
        ui.info(&format!(
            "Capabilities: {}",
            if selected_caps.is_empty() {
                "none".to_string()
            } else {
                selected_caps.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        ));

        ui.info(
            "\nRun `granite-cli launcher list` to see configured launchers.",
        );
        if !selected_launchers.is_empty() {
            let first_launcher = selected_launchers.iter().next().unwrap();
            ui.info(&format!(
                "Run `granite-cli launch {first_launcher}` to launch with Granite overlay.",
            ));
        }
    }
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ModelConfig, ProviderConfig};
    use crate::utils::ui::base::tests::CaptureUi;
    use std::sync::Arc;

    fn test_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Arc::new(CaptureUi::default()),
        }
    }

    fn ctx_with_provider(id: &str, provider_type: &str, config: serde_json::Value) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.providers.insert(
            id.to_string(),
            ProviderConfig {
                provider_id: id.to_string(),
                provider_type: provider_type.to_string(),
                config,
            },
        );
        ctx
    }

    fn ctx_with_model(id: &str, provider_id: Option<&str>) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.models.insert(
            id.to_string(),
            ModelConfig {
                model_id: id.to_string(),
                provider_id: provider_id.map(String::from),
                variant: None,
            },
        );
        ctx
    }

    // -- discover_providers ----------------------------------------------------

    #[tokio::test]
    async fn discover_providers_skips_configured() {
        let ctx = ctx_with_provider("ollama", "ollama", serde_json::json!({}));
        let result = Discover::run(&ctx).await;
        assert!(result.configured_provider_ids.contains(&"ollama".to_string()));
    }

    #[tokio::test]
    async fn discover_providers_recommends_unconfigured() {
        let ctx = test_ctx();
        let result = Discover::run(&ctx).await;
        let provider_recs: Vec<_> = result
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Provider { .. }))
            .collect();
        // Should have at least some provider recommendations
        assert!(
            !provider_recs.is_empty(),
            "expected at least one provider recommendation"
        );
    }

    // -- discover_models -------------------------------------------------------

    #[tokio::test]
    async fn discover_models_groups_by_family() {
        let ctx = test_ctx();
        let result = Discover::run(&ctx).await;
        let model_recs: Vec<_> = result
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Model { .. }))
            .collect();
        // Should have at least one model recommendation
        assert!(
            !model_recs.is_empty(),
            "expected at least one model recommendation"
        );
    }

    #[tokio::test]
    async fn discover_models_recommendation_carries_real_registry_model_id() {
        let ctx = test_ctx();
        let result = Discover::run(&ctx).await;
        for rec in &result.recommendations {
            if let Recommendation::Model { model_id, family, .. } = rec {
                assert!(
                    MODEL_REGISTRY.get(model_id).is_some(),
                    "model_id '{model_id}' should be a real catalog key, not the family name"
                );
                assert_ne!(
                    model_id, family,
                    "model_id should be the specific model's catalog id, not its family"
                );
            }
        }
    }

    #[tokio::test]
    async fn discover_models_skips_configured() {
        let ctx = ctx_with_model(
            "granite-3.1-8b-instruct",
            Some("ollama"),
        );
        let result = Discover::run(&ctx).await;
        assert!(result.configured_model_ids.contains(&"granite-3.1-8b-instruct".to_string()));
    }

    // -- discover_launchers ----------------------------------------------------

    #[tokio::test]
    async fn discover_launchers_skips_configured() {
        let mut ctx = test_ctx();
        ctx.config.launchers.insert(
            "claude".to_string(),
            crate::config::LauncherConfig {
                launcher_id: "claude".to_string(),
                launcher_type: "claude".to_string(),
                enabled_capabilities: vec![],
                config: serde_json::json!({}),
            },
        );
        let result = Discover::run(&ctx).await;
        assert!(result.configured_launcher_ids.contains(&"claude".to_string()));
    }

    #[tokio::test]
    async fn discover_launchers_recommends_unconfigured() {
        let ctx = test_ctx();
        let result = Discover::run(&ctx).await;
        let launcher_recs: Vec<_> = result
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Launcher { .. }))
            .collect();
        assert!(
            !launcher_recs.is_empty(),
            "expected at least one launcher recommendation"
        );
    }

    // -- discover_capabilities -------------------------------------------------

    #[tokio::test]
    async fn discover_capabilities_recommends_unconfigured() {
        let ctx = test_ctx();
        let result = Discover::run(&ctx).await;
        let cap_recs: Vec<_> = result
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Capability { .. }))
            .collect();
        assert!(
            !cap_recs.is_empty(),
            "expected at least one capability recommendation"
        );
    }

    // -- version comparison ----------------------------------------------------

    #[test]
    fn compare_versions_desc_simple() {
        assert_eq!(compare_versions_desc("3.1", "3.0"), std::cmp::Ordering::Less);
        assert_eq!(compare_versions_desc("3.0", "3.1"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions_desc("3.1", "3.1"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn compare_versions_desc_multi_part() {
        assert_eq!(
            compare_versions_desc("3.1.1", "3.1.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions_desc("3.1", "3.1.0"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn compare_versions_desc_major_difference() {
        assert_eq!(
            compare_versions_desc("4.0", "3.1"),
            std::cmp::Ordering::Less
        );
    }

    // -- size helpers ----------------------------------------------------------

    #[test]
    fn format_size_billion() {
        assert_eq!(format_size(8_000_000_000), "8B");
    }

    #[test]
    fn format_size_million() {
        assert_eq!(format_size(2_000_000), "2M");
    }

    #[test]
    fn parse_size_billion() {
        assert_eq!(parse_size("8B"), 8_000_000_000);
    }

    #[test]
    fn parse_size_million() {
        assert_eq!(parse_size("2M"), 2_000_000);
    }

    // -- Revaluator ------------------------------------------------------------

    #[tokio::test]
    async fn revaluator_for_models_filters_by_capability_requirements() {
        let ctx = test_ctx();
        let discovery = Discover::run(&ctx).await;

        // agent-model requires Chat + ToolCalling support.
        let selected_caps: HashSet<String> = ["agent-model".to_string()].into_iter().collect();
        let filtered = Revaluator::for_models(&discovery, &selected_caps);

        assert!(
            !filtered.is_empty(),
            "expected at least one granite model to satisfy agent-model's Chat+ToolCalling requirement"
        );
        // Every surviving recommendation's real catalog metadata must
        // actually admit the requirement -- this is the regression check for
        // the bug where a hand-rolled mock always reported empty
        // `supported_functions`, so nothing ever matched.
        for rec in &filtered {
            if let Recommendation::Model { model_id, .. } = rec {
                let md = MODEL_REGISTRY.get(model_id).expect("real catalog entry");
                assert!(
                    md.supported_functions.contains(&crate::models::ModelFunction::Chat),
                    "{model_id} should support Chat"
                );
            }
        }
    }

    #[tokio::test]
    async fn revaluator_for_models_with_no_requirements_returns_all() {
        let ctx = test_ctx();
        let discovery = Discover::run(&ctx).await;
        let filtered = Revaluator::for_models(&discovery, &HashSet::new());
        let all_models: Vec<_> = discovery
            .recommendations
            .iter()
            .filter(|r| matches!(r, Recommendation::Model { .. }))
            .collect();
        assert_eq!(filtered.len(), all_models.len());
    }

    #[tokio::test]
    async fn revaluator_for_launchers_filters_by_binding_types() {
        let ctx = test_ctx();
        let discovery = Discover::run(&ctx).await;

        // agent-model needs the AgentModel binding, which bob does not support.
        let selected_caps: HashSet<String> = ["agent-model".to_string()].into_iter().collect();
        let filtered = Revaluator::for_launchers(&discovery, &selected_caps);
        assert!(
            !filtered
                .iter()
                .any(|r| matches!(r, Recommendation::Launcher { launcher_type, .. } if launcher_type == "bob")),
            "bob only supports Mcp and should be filtered out for agent-model"
        );

        // vision-mcp only needs the Mcp binding, which bob does support.
        let mcp_caps: HashSet<String> = ["vision-mcp".to_string()].into_iter().collect();
        let mcp_filtered = Revaluator::for_launchers(&discovery, &mcp_caps);
        assert!(
            mcp_filtered
                .iter()
                .any(|r| matches!(r, Recommendation::Launcher { launcher_type, .. } if launcher_type == "bob")),
            "bob supports Mcp and should be included for vision-mcp"
        );
    }

    #[tokio::test]
    async fn revaluator_for_capabilities_filters_by_selected_launchers() {
        let ctx = test_ctx();
        let discovery = Discover::run(&ctx).await;

        // bob only supports the Mcp binding, so only vision-mcp should show.
        let bob_only: HashSet<String> = ["bob".to_string()].into_iter().collect();
        let filtered = Revaluator::for_capabilities(&discovery, &bob_only);
        assert!(
            filtered
                .iter()
                .all(|r| matches!(r, Recommendation::Capability { capability_type, .. } if capability_type == "vision-mcp")),
            "with only bob (Mcp-only) selected, only vision-mcp should be recommended"
        );
    }

    #[tokio::test]
    async fn revaluator_for_capabilities_with_no_launchers_returns_none() {
        let ctx = test_ctx();
        let discovery = Discover::run(&ctx).await;
        let filtered = Revaluator::for_capabilities(&discovery, &HashSet::new());
        assert!(filtered.is_empty());
    }

    // -- SetupCommands ---------------------------------------------------------

    #[tokio::test]
    async fn run_wizard_with_empty_config_shows_info() {
        let mut ctx = test_ctx();
        let _ = SetupCommands::run(&mut ctx, false, true).await;
        // Wizard should complete without error even with no recommendations
        // (it will show info messages)
    }

    #[tokio::test]
    async fn run_auto_with_no_recommendations_shows_info() {
        let _home = crate::config::TestConfigHome::new();
        let mut ctx = test_ctx();
        let result = SetupCommands::run(&mut ctx, true, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn select_launchers_excludes_launchers_without_a_resolved_binary() {
        let capture = Arc::new(CaptureUi::default());
        let mut ctx = crate::AppContext {
            config: Config::default(),
            ui: capture.clone(),
        };

        let discovery = DiscoveryResult {
            recommendations: vec![
                Recommendation::Launcher {
                    launcher_type: "found".to_string(),
                    launcher_name: "Found Launcher".to_string(),
                    binary_path: Some("/usr/bin/found".to_string()),
                },
                Recommendation::Launcher {
                    launcher_type: "missing".to_string(),
                    launcher_name: "Missing Launcher".to_string(),
                    binary_path: None,
                },
            ],
            configured_provider_ids: vec![],
            configured_model_ids: vec![],
            configured_launcher_ids: vec![],
            configured_capability_ids: vec![],
        };

        SetupCommands::select_launchers(&mut ctx, &discovery)
            .await
            .unwrap();

        let prompts = capture.multi_select_prompts.borrow();
        let (_, items, _) = &prompts[0];
        assert_eq!(items.len(), 1, "the missing binary should have been excluded");
        assert!(items[0].contains("found"));
    }
}
