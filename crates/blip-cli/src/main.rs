pub mod daemon_client;
mod store_backend;

use blip_config::BlipConfig;
use blip_core::{BlipError, ContentType, NewBlip, NewWorkspace};
use clap::{Parser, Subcommand, ValueEnum};
use daemon_client::DaemonClient;
use std::io;
use store_backend::StoreCommandBackend;

const DEFAULT_LIST_LIMIT: usize = 50;

#[derive(Debug, Parser)]
#[command(name = "blip")]
#[command(about = "CLI for interacting with the blipcoard runtime", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Health {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Current {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Workspaces {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Inbox {
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    List {
        workspace: String,
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        agent_access: bool,
    },
    Use {
        workspace: String,
    },
    AddDemo {
        workspace: String,
        content: String,
        #[arg(long)]
        source_app: Option<String>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = BlipConfig::load_or_create()?;

    match cli.command {
        Commands::Health { output } => {
            let health = DaemonClient::from_config(&config)?.health()?;
            match output {
                OutputFormat::Human => {
                    println!("{} {}", health.service, health.status);
                    println!("database: {}", health.database_path);
                    let active_workspace = health.active_workspace.as_deref().unwrap_or("none");
                    println!("active workspace: {active_workspace}");
                    println!("generated at: {}", health.generated_at);
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &health)?;
                    println!();
                }
            }
        }
        Commands::Current { output } => {
            let current = DaemonClient::from_config(&config)?.current_workspace()?;
            match output {
                OutputFormat::Human => {
                    let active_workspace = current.active_workspace.as_deref().unwrap_or("none");
                    println!("{active_workspace}");
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &current)?;
                    println!();
                }
            }
        }
        Commands::Workspaces { output } => {
            let workspaces = DaemonClient::from_config(&config)?.workspaces()?;
            match output {
                OutputFormat::Human => {
                    for workspace in workspaces.workspaces {
                        let access = if workspace.agent_access {
                            "agent-readable"
                        } else {
                            "human-only"
                        };
                        println!("{} [{access}]", workspace.name);
                    }
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &workspaces)?;
                    println!();
                }
            }
        }
        Commands::Inbox { limit, output } => {
            let blips = DaemonClient::from_config(&config)?.blips("inbox", limit)?;
            match output {
                OutputFormat::Human => {
                    for blip in blips.blips {
                        println!(
                            "{} :: {}",
                            blip.id,
                            format_preview(&blip.preview, blip.size_bytes)
                        );
                    }
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &blips)?;
                    println!();
                }
            }
        }
        Commands::List {
            workspace,
            limit,
            output,
        } => {
            let blips = DaemonClient::from_config(&config)?.blips(&workspace, limit)?;
            match output {
                OutputFormat::Human => {
                    for blip in blips.blips {
                        println!(
                            "{} :: {}",
                            blip.id,
                            format_preview(&blip.preview, blip.size_bytes)
                        );
                    }
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &blips)?;
                    println!();
                }
            }
        }
        Commands::Create {
            name,
            description,
            color,
            agent_access,
        } => {
            let mut store_backend = StoreCommandBackend::open(&config.database_path)?;
            let workspace = match store_backend.create_workspace(&NewWorkspace {
                name,
                description,
                color,
                agent_access,
                sticky_capture: false,
                retention_days: None,
            }) {
                Ok(workspace) => workspace,
                Err(BlipError::WorkspaceAlreadyExists(name)) => {
                    return Err(format!("workspace `{name}` already exists").into());
                }
                Err(error) => return Err(error.into()),
            };
            println!("created workspace {}", workspace.name);
        }
        Commands::Use { workspace } => {
            let mut store_backend = StoreCommandBackend::open(&config.database_path)?;
            store_backend.set_active_workspace(&workspace)?;
            println!("active workspace set to {workspace}");
        }
        Commands::AddDemo {
            workspace,
            content,
            source_app,
        } => {
            let mut store_backend = StoreCommandBackend::open(&config.database_path)?;
            let blip = store_backend.insert_blip(&NewBlip {
                workspace_name: workspace,
                source_app,
                content_type: ContentType::PlainText,
                language: None,
                content,
                token_estimate: None,
                is_redacted: false,
                tags: vec!["demo".to_string()],
            })?;
            println!("created blip {}", blip.id);
        }
    }

    Ok(())
}

fn format_preview(preview: &str, size_bytes: i64) -> String {
    if i64::try_from(preview.len()).is_ok_and(|preview_len| preview_len < size_bytes) {
        format!("{preview}...")
    } else {
        preview.to_owned()
    }
}
