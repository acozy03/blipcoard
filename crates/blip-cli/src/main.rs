use blip_config::BlipConfig;
use blip_core::{BlipStore, ContentType, NewBlip, NewWorkspace};
use clap::{Parser, Subcommand};

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
    Inbox,
    List {
        workspace: String,
    },
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        color: Option<String>,
        #[arg(long, default_value_t = true)]
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
    let store = BlipStore::open(config.database_path.to_string_lossy().as_ref())?;

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
        Commands::Inbox => {
            for blip in store.list_blips("inbox")? {
                println!("{} :: {}", blip.id, summarize(&blip.content));
            }
        }
        Commands::List { workspace } => {
            for blip in store.list_blips(&workspace)? {
                println!("{} :: {}", blip.id, summarize(&blip.content));
            }
        }
        Commands::Create {
            name,
            description,
            color,
            agent_access,
        } => {
            let workspace = store.create_workspace(&NewWorkspace {
                name,
                description,
                color,
                agent_access,
                sticky_capture: false,
                retention_days: None,
            })?;
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

fn summarize(content: &str) -> String {
    const MAX_LEN: usize = 72;
    if content.len() <= MAX_LEN {
        return content.to_string();
    }

    let end = content
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_LEN)
        .last()
        .unwrap_or(0);

    format!("{}...", &content[..end])
}
