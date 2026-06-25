pub mod config;
pub mod registry;
pub mod capabilities;
pub mod commands;
pub mod utils;

use clap::{Parser, Subcommand};
use commands::{ModelCommands, CapabilityCommands};

#[derive(Parser, Debug)]
#[command(name = "granite-cli")]
#[command(about = "Offload work to Granite models, harden skills, launch agents", long_about = None)]
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

    /// Provider management (Phase 2)
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

    /// REPL chatbot (Phase 2)
    Run {
        /// Optional model ID
        model_id: Option<String>,

        /// Launch TUI mode
        #[arg(short, long)]
        tui: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ModelSubcommands {
    /// List all available models
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Model(subcmd)) => run_model_command(subcmd).await,
        Some(Commands::Capability(subcmd)) => run_capability_command(subcmd).await,
        Some(Commands::Provider(_)) => {
            println!("Provider management will be available in Phase 2.");
            Ok(())
        }
        Some(Commands::Configure(args)) => run_configure(args).await,
        Some(Commands::Launch { .. }) => {
            println!("Tool launching will be available in Phase 3.");
            Ok(())
        }
        Some(Commands::Run { .. }) => {
            println!("Chatbot will be available in Phase 2.");
            Ok(())
        }
        None => {
            println!("granite-cli - Universal Model Adapter with Capabilities");
            println!("\nUsage: granite-cli <command> [subcommand] [options]");
            println!("\nAvailable commands:");
            println!("  model        Model management (list, info, setup)");
            println!("  capability   Capability management (list, info, setup)");
            println!("  provider     Provider management (Phase 2)");
            println!("  configure    Configure tools (Phase 2-3)");
            println!("  launch       Launch tool with overlay (Phase 3)");
            println!("  run          REPL chatbot (Phase 2)");
            println!("\nTry 'granite-cli model list' to get started.");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run_model_command(subcmd: ModelSubcommands) -> anyhow::Result<()> {
    match subcmd {
        ModelSubcommands::List { r#type } => {
            let filter = match r#type.as_deref() {
                Some("text") => Some(registry::ModelType::Text),
                Some("vision") => Some(registry::ModelType::Vision),
                Some("speech") => Some(registry::ModelType::Speech),
                Some("embedding") => Some(registry::ModelType::Embedding),
                Some(t) => {
                    anyhow::bail!("Unknown model type: {}. Valid types: text, vision, speech, embedding", t);
                }
                None => None,
            };
            ModelCommands::list(filter)
        }
        ModelSubcommands::Info { model_id } => ModelCommands::info(&model_id),
        ModelSubcommands::Setup { model_id } => ModelCommands::setup(&model_id),
    }
}

async fn run_capability_command(subcmd: CapabilitySubcommands) -> anyhow::Result<()> {
    match subcmd {
        CapabilitySubcommands::List => CapabilityCommands::list(),
        CapabilitySubcommands::Info { capability_id } => CapabilityCommands::info(&capability_id),
        CapabilitySubcommands::Setup { capability_id } => CapabilityCommands::setup(&capability_id),
    }
}

async fn run_configure(_args: ConfigureArgs) -> anyhow::Result<()> {
    println!("Tool configuration wizard will be available in Phase 3.");
    Ok(())
}
