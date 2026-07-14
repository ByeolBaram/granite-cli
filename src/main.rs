pub mod config;
pub mod registry;
pub mod models;
pub mod capabilities;
pub mod commands;
pub mod utils;
// TODO: Re-enable once rewritten -- pub mod di;
pub mod providers;

// Third Party
use clap::{Parser, Subcommand};

// Local
use commands::{CapabilityCommands, ModelCommands, ProviderCommands};

// Hoist paste macro for use in our own macros
extern crate paste;

#[derive(Parser, Debug)]
#[command(name = "granite-cli")]
#[command(about = "Universal Model Adapter with Capabilities", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Model management commands
    #[command(subcommand)]
    Model(ModelSubcommands),

    /// Capability management commands
    #[command(subcommand)]
    Capability(CapabilitySubcommands),

    /// Provider management commands
    #[command(subcommand)]
    Provider(ProviderSubcommands),

    /// Configure tools with Granite capabilities
    Configure(ConfigureArgs),

    /// Launch a tool with Granite overlay
    Launch {
        /// Tool ID to launch
        tool_id: String,

        /// Show overlay without launching
        #[arg(long)]
        dry_run: bool,

        /// Additional arguments to pass to the tool
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
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
}

#[derive(Subcommand, Debug)]
enum CapabilitySubcommands {
    /// List all available capabilities
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
    /// List configured providers
    List,

    /// Interactive provider setup wizard
    Setup {
        /// Provider ID to set up
        provider_id: String,
    },

    /// Check provider health
    Health {
        /// Provider ID (optional, checks all if not specified)
        provider_id: Option<String>,
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
}

impl AppContext {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            config: config::Config::new()?,
        })
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Model(subcmd)) => {
            let mut ctx = AppContext::new().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(1);
            });
            run_model_command(&mut ctx, subcmd).await
        }
        Some(Commands::Capability(subcmd)) => {
            let mut ctx = AppContext::new().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(1);
            });
            run_capability_command(&mut ctx, subcmd).await
        }
        Some(Commands::Provider(subcmd)) => {
            let mut ctx = AppContext::new().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(1);
            });
            run_provider_command(&mut ctx, subcmd).await
        }
        Some(Commands::Configure(args)) => run_configure(args).await,
        Some(Commands::Launch { .. }) => {
            println!("Tool launching will be available in Phase 3.");
            Ok(())
        }
        None => {
            println!("granite-cli - Universal Model Adapter with Capabilities");
            println!();
            println!("Usage: granite-cli <command> [subcommand] [options]");
            println!();
            println!("Available commands:");
            println!("  model        Model management (catalog, list, info, setup)");
            println!("  capability   Capability management (list, info, setup)");
            println!("  provider     Provider management (list, setup, health)");
            println!("  configure    Configure tools (Phase 3)");
            println!("  launch       Launch tool with overlay (Phase 3)");
            println!();
            println!("Try 'granite-cli model list' to get started.");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
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
                    anyhow::bail!("Unknown model type: {}. Valid types: text, vision, speech, embedding", t);
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
                    anyhow::bail!("Unknown model type: {}. Valid types: text, vision, speech, embedding", t);
                }
                None => None,
            };
            ModelCommands::list(ctx, filter)
        }
        ModelSubcommands::Info { model_id } => ModelCommands::info(ctx, &model_id),
        ModelSubcommands::Setup { model_id } => ModelCommands::setup(ctx, &model_id),
    }
}

async fn run_capability_command(ctx: &mut AppContext, subcmd: CapabilitySubcommands) -> anyhow::Result<()> {
    match subcmd {
        CapabilitySubcommands::List => CapabilityCommands::list(ctx),
        CapabilitySubcommands::Info { capability_id } => CapabilityCommands::info(ctx, &capability_id),
        CapabilitySubcommands::Setup { capability_id } => CapabilityCommands::setup(ctx, &capability_id).await,
    }
}

async fn run_provider_command(ctx: &mut AppContext, subcmd: ProviderSubcommands) -> anyhow::Result<()> {
    match subcmd {
        ProviderSubcommands::List => ProviderCommands::list(ctx),
        ProviderSubcommands::Setup { provider_id } => ProviderCommands::setup(ctx, &provider_id).await,
        ProviderSubcommands::Health { provider_id } => ProviderCommands::health(ctx, provider_id.as_deref()).await,
    }
}

async fn run_configure(_args: ConfigureArgs) -> anyhow::Result<()> {
    println!("Tool configuration wizard will be available in Phase 3.");
    Ok(())
}
