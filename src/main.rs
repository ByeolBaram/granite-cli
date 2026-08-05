pub mod capabilities;
pub mod commands;
pub mod config;
pub mod dependency;
pub mod launchers;
pub mod models;
pub mod registry;
pub mod utils;
// TODO: Re-enable once rewritten -- pub mod di;
pub mod providers;

// Third Party
use clap::{Parser, Subcommand};

// Local
use commands::{
    CapabilityCommands, HardwareCommands, LauncherCommands, ModelCommands, ProviderCommands,
};
use utils::ui::{UI_REGISTRY, Ui, run_interactive_tui};

// Hoist paste macro for use in our own macros
extern crate paste;

#[derive(Parser, Debug)]
#[command(name = "granite-cli")]
#[command(about = "Universal Model Adapter with Capabilities", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Args, Debug)]
struct ModelWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    #[command(subcommand)]
    subcommand: ModelSubcommands,
}

#[derive(clap::Args, Debug)]
struct CapabilityWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    #[command(subcommand)]
    subcommand: CapabilitySubcommands,
}

#[derive(clap::Args, Debug)]
struct ProviderWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    #[command(subcommand)]
    subcommand: ProviderSubcommands,
}

#[derive(clap::Args, Debug)]
struct ConfigureWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    #[command(flatten)]
    args: ConfigureArgs,
}

#[derive(clap::Args, Debug)]
struct LaunchWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    /// Tool ID to launch
    tool_id: String,

    /// Show overlay without launching
    #[arg(long)]
    dry_run: bool,

    /// Additional arguments to pass to the tool
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct LauncherWithOutput {
    /// Output format: terminal (default), plain, json, markdown
    #[arg(short, long, global = true, default_value = "terminal")]
    output: String,

    #[command(subcommand)]
    subcommand: LauncherSubcommands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Model management commands
    Model(ModelWithOutput),

    /// Capability management commands
    Capability(CapabilityWithOutput),

    /// Provider management commands
    Provider(ProviderWithOutput),

    /// Launcher management commands
    Launcher(LauncherWithOutput),

    /// Show hardware profile and recommended precision
    Hardware,

    /// Configure tools with Granite capabilities
    Configure(ConfigureWithOutput),

    /// Launch a tool with Granite overlay
    Launch(LaunchWithOutput),
}

#[derive(Subcommand, Debug)]
enum ModelSubcommands {
    /// Show the catalog of all available models
    Catalog {
        /// Filter by model type
        #[arg(short, long)]
        r#type: Option<String>,
    },

    /// List all configured models
    List {
        /// Filter by model type
        #[arg(short, long)]
        r#type: Option<String>,
    },

    /// Search the model catalog by ID or family
    Search {
        /// Case-insensitive substring to search for
        query: String,
    },

    /// Recommend models that fit current hardware
    Recommend {
        /// Filter by model type
        #[arg(short, long)]
        r#type: Option<String>,

        /// Configured provider id(s) to check against (comma-separated or
        /// repeatable), or "all" to skip the provider check and show every
        /// model that fits the hardware regardless of configured providers
        #[arg(short = 'p', long = "providers", value_delimiter = ',')]
        providers: Vec<String>,

        /// Show all columns, including family and full context length
        #[arg(long)]
        wide: bool,
    },

    /// Show detailed model information
    Info {
        /// Model ID
        model_id: String,
    },

    /// Interactive model setup wizard
    Setup {
        /// Model ID to set up
        model_id: String,
    },

    /// Pull (download) a configured model's weights via its provider
    Pull {
        /// Model ID to pull
        model_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum CapabilitySubcommands {
    /// Show the catalog of all available capabilities
    Catalog,

    /// List all configured capabilities
    List,

    /// Show detailed capability information
    Info {
        /// Capability ID
        capability_id: String,
    },

    /// Set up a capability
    Setup {
        /// Capability ID to set up
        capability_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderSubcommands {
    /// Show the catalog of all available providers
    Catalog,

    /// List all configured providers
    List,

    /// Interactive provider setup wizard
    Setup {
        /// Catalog provider type to set up (e.g. `openai-compatible`)
        provider_type: String,

        /// Nickname for this provider instance. Defaults to `provider_type`;
        /// pass a distinct value to configure multiple named instances of
        /// the same catalog type (e.g. `--id ollama`, `--id lm-studio`).
        #[arg(long = "id")]
        instance_id: Option<String>,
    },

    /// Check provider health
    Health {
        /// Provider ID (optional, checks all if not specified)
        provider_id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum LauncherSubcommands {
    /// Show the catalog of all available launcher types
    Catalog,

    /// List all configured launcher instances
    List,

    /// Interactive launcher setup wizard
    Setup {
        /// Catalog launcher type to set up (e.g. `claude`)
        launcher_type: String,

        /// Nickname for this launcher instance. Defaults to `launcher_type`;
        /// pass a distinct value to configure multiple named instances of
        /// the same catalog type (e.g. `--id claude-local`).
        #[arg(long = "id")]
        instance_id: Option<String>,
    },

    /// Remove a configured launcher instance
    Remove {
        /// Configured launcher instance ID to remove
        launcher_id: String,
    },
}

#[derive(clap::Args, Debug)]
struct ConfigureArgs {
    /// Tool ID to configure
    tool_id: String,

    /// Export configuration to shell profile
    #[arg(long)]
    export: bool,

    /// Reset tool configuration
    #[arg(long)]
    reset: bool,
}

pub struct AppContext {
    pub config: config::Config,
    pub ui: Box<dyn Ui>,
}

/// Construct the `Ui` backend for `--output`, exiting on an unrecognized
/// format. No `Ui` exists yet at this point, so this is the one place in
/// `main` that still reports via `eprintln!` rather than `ctx.ui`.
fn construct_ui(output: &str) -> Box<dyn Ui> {
    UI_REGISTRY
        .construct(output, &serde_json::json!({}))
        .unwrap_or_else(|_| {
            eprintln!("Unknown output format '{output}'. Valid: terminal, plain, json, markdown");
            std::process::exit(1);
        })
}

fn construct_context(output: &str) -> AppContext {
    let ui = construct_ui(output);
    let config = config::Config::new().unwrap_or_else(|e| {
        ui.error(&format!("Failed to load config: {e}"));
        std::process::exit(1);
    });
    AppContext { config, ui }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result: Result<(), ()> = match cli.command {
        Some(Commands::Model(wrapper)) => {
            let mut ctx = construct_context(&wrapper.output);
            run_model_command(&mut ctx, wrapper.subcommand)
                .await
                .map_err(|e| ctx.ui.error(&e.to_string()))
        }
        Some(Commands::Capability(wrapper)) => {
            let mut ctx = construct_context(&wrapper.output);
            run_capability_command(&mut ctx, wrapper.subcommand)
                .await
                .map_err(|e| ctx.ui.error(&e.to_string()))
        }
        Some(Commands::Provider(wrapper)) => {
            let mut ctx = construct_context(&wrapper.output);
            run_provider_command(&mut ctx, wrapper.subcommand)
                .await
                .map_err(|e| ctx.ui.error(&e.to_string()))
        }
        Some(Commands::Hardware) => {
            let ctx = construct_context("terminal");
            HardwareCommands::show(&ctx).map_err(|e| ctx.ui.error(&e.to_string()))
        }
        Some(Commands::Configure(wrapper)) => {
            let ui = construct_ui(&wrapper.output);
            run_configure(&*ui, wrapper.args)
                .await
                .map_err(|e| ui.error(&e.to_string()))
        }
        Some(Commands::Launcher(wrapper)) => {
            let mut ctx = construct_context(&wrapper.output);
            run_launcher_command(&mut ctx, wrapper.subcommand)
                .await
                .map_err(|e| ctx.ui.error(&e.to_string()))
        }
        Some(Commands::Launch(wrapper)) => {
            let ctx = construct_context(&wrapper.output);
            run_launch(&*ctx.ui, &wrapper.tool_id, &wrapper.args, wrapper.dry_run)
                .await
                .map_err(|e| ctx.ui.error(&e.to_string()))
        }
        None => {
            // `ctx` (and its `ui`) is consumed by value into the TUI `App`
            // before any error can occur, so it can't be used to report one.
            let ctx = construct_context("terminal");
            run_interactive_tui(ctx)
                .await
                .map_err(|e| eprintln!("Error: {e}"))
        }
    };

    if result.is_err() {
        std::process::exit(1);
    }
}

async fn run_model_command(ctx: &mut AppContext, subcmd: ModelSubcommands) -> anyhow::Result<()> {
    match subcmd {
        ModelSubcommands::Catalog { r#type } => {
            let filter = match r#type.as_deref() {
                Some("text") => Some(models::ModelType::Text),
                Some("vision") => Some(models::ModelType::Vision),
                Some("speech") => Some(models::ModelType::Speech),
                Some("embedding") => Some(models::ModelType::Embedding),
                Some(t) => {
                    anyhow::bail!(
                        "Unknown model type: {t}. Valid types: text, vision, speech, embedding"
                    );
                }
                None => None,
            };
            ModelCommands::catalog(ctx, filter)
        }
        ModelSubcommands::List { r#type } => {
            let filter = match r#type.as_deref() {
                Some("text") => Some(models::ModelType::Text),
                Some("vision") => Some(models::ModelType::Vision),
                Some("speech") => Some(models::ModelType::Speech),
                Some("embedding") => Some(models::ModelType::Embedding),
                Some(t) => {
                    anyhow::bail!(
                        "Unknown model type: {t}. Valid types: text, vision, speech, embedding"
                    );
                }
                None => None,
            };
            ModelCommands::list(ctx, filter)
        }
        ModelSubcommands::Search { query } => ModelCommands::search(ctx, &query),
        ModelSubcommands::Recommend {
            r#type,
            providers,
            wide,
        } => {
            let filter = match r#type.as_deref() {
                Some("text") => Some(models::ModelType::Text),
                Some("vision") => Some(models::ModelType::Vision),
                Some("speech") => Some(models::ModelType::Speech),
                Some("embedding") => Some(models::ModelType::Embedding),
                Some(t) => {
                    anyhow::bail!(
                        "Unknown model type: {t}. Valid types: text, vision, speech, embedding"
                    );
                }
                None => None,
            };
            ModelCommands::recommend(ctx, filter, &providers, wide)
        }
        ModelSubcommands::Info { model_id } => ModelCommands::info(ctx, &model_id),
        ModelSubcommands::Setup { model_id } => ModelCommands::setup(ctx, &model_id).await,
        ModelSubcommands::Pull { model_id } => ModelCommands::pull(ctx, &model_id).await,
    }
}

async fn run_capability_command(
    ctx: &mut AppContext,
    subcmd: CapabilitySubcommands,
) -> anyhow::Result<()> {
    match subcmd {
        CapabilitySubcommands::Catalog => CapabilityCommands::catalog(ctx),
        CapabilitySubcommands::List => CapabilityCommands::list(ctx),
        CapabilitySubcommands::Info { capability_id } => {
            CapabilityCommands::info(ctx, &capability_id)
        }
        CapabilitySubcommands::Setup { capability_id } => {
            CapabilityCommands::setup(ctx, &capability_id).await
        }
    }
}

async fn run_provider_command(
    ctx: &mut AppContext,
    subcmd: ProviderSubcommands,
) -> anyhow::Result<()> {
    match subcmd {
        ProviderSubcommands::Catalog => ProviderCommands::catalog(ctx),
        ProviderSubcommands::List => ProviderCommands::list(ctx),
        ProviderSubcommands::Setup {
            provider_type,
            instance_id,
        } => ProviderCommands::setup(ctx, &provider_type, instance_id.as_deref()).await,
        ProviderSubcommands::Health { provider_id } => {
            ProviderCommands::health(ctx, provider_id.as_deref()).await
        }
    }
}

async fn run_configure(ui: &dyn Ui, _args: ConfigureArgs) -> anyhow::Result<()> {
    ui.info("Tool configuration wizard will be available in Phase 3.");
    Ok(())
}

async fn run_launcher_command(
    ctx: &mut AppContext,
    subcmd: LauncherSubcommands,
) -> anyhow::Result<()> {
    match subcmd {
        LauncherSubcommands::Catalog => LauncherCommands::catalog(ctx),
        LauncherSubcommands::List => LauncherCommands::list(ctx),
        LauncherSubcommands::Setup {
            launcher_type,
            instance_id,
        } => LauncherCommands::setup(ctx, &launcher_type, instance_id.as_deref()).await,
        LauncherSubcommands::Remove { launcher_id } => LauncherCommands::remove(ctx, &launcher_id),
    }
}

async fn run_launch(
    ui: &dyn Ui,
    launcher_id: &str,
    args: &[String],
    dry_run: bool,
) -> anyhow::Result<()> {
    use crate::launchers::LAUNCHER_REGISTRY;
    use crate::launchers::LaunchContext;

    // Load config fresh so we always pick up the latest saved state.
    let config = crate::config::Config::new()?;

    let lc = config.get_launcher(launcher_id).ok_or_else(|| {
        anyhow::anyhow!(
            "No launcher configured with id '{launcher_id}'. \
             Run `granite-cli launcher setup {launcher_id}` first."
        )
    })?;

    let launcher = LAUNCHER_REGISTRY
        .construct(&lc.launcher_type, &lc.config)
        .map_err(|e| anyhow::anyhow!("Failed to construct launcher: {e}"))?;

    let launch_ctx = LaunchContext {
        launcher_id: launcher_id.to_string(),
        working_dir: std::env::current_dir()?,
        base_env: std::collections::HashMap::new(),
        dry_run,
    };

    let status = launcher.launch(args, &launch_ctx, ui).await?;
    if !status.success() {
        anyhow::bail!(
            "'{}' exited with status {}",
            launcher_id,
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}
