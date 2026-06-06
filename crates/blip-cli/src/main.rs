use blip_config::BlipConfig;
use blip_core::{BlipError, BlipStore, ContentType, NewBlip, NewWorkspace};
use clap::{Parser, Subcommand};

const DEFAULT_LIST_LIMIT: usize = 50;

#[derive(Debug, Parser)]
#[command(name = "blip")]
#[command(about = "CLI for interacting with the blipcoard runtime", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Current,
    Workspaces,
    Inbox {
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
    },
    List {
        workspace: String,
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = BlipConfig::load_or_create()?;
    let mut store = BlipStore::open(&config.database_path)?;

    match cli.command {
        Commands::Current => {
            let current = store
                .get_active_workspace()?
                .unwrap_or_else(|| "none".to_string());
            println!("{current}");
        }
        Commands::Workspaces => {
            for workspace in store.list_workspaces()? {
                let access = if workspace.agent_access {
                    "agent-readable"
                } else {
                    "human-only"
                };
                println!("{} [{access}]", workspace.name);
            }
        }
        Commands::Inbox { limit } => {
            for blip in store.list_blip_summaries("inbox", limit)? {
                println!(
                    "{} :: {}",
                    blip.id,
                    format_preview(&blip.preview, blip.size_bytes)
                );
            }
        }
        Commands::List { workspace, limit } => {
            for blip in store.list_blip_summaries(&workspace, limit)? {
                println!(
                    "{} :: {}",
                    blip.id,
                    format_preview(&blip.preview, blip.size_bytes)
                );
            }
        }
        Commands::Create {
            name,
            description,
            color,
            agent_access,
        } => {
            let workspace = match store.create_workspace(&NewWorkspace {
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
            store.set_active_workspace(&workspace)?;
            println!("active workspace set to {workspace}");
        }
        Commands::AddDemo {
            workspace,
            content,
            source_app,
        } => {
            let blip = store.insert_blip(&NewBlip {
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
