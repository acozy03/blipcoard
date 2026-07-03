pub mod daemon_client;
mod service;
mod store_backend;

use blip_api::{PayloadRequester, PayloadSummary, WorkspaceSummary};
use blip_config::BlipConfig;
use blip_core::{BlipError, ContentType, NewBlip, NewWorkspace};
use clap::{Parser, Subcommand, ValueEnum};
use daemon_client::DaemonClient;
use service::{ServicePlan, render_status_json};
use std::io;
use std::path::{Path, PathBuf};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum RichPayloadVisibilityArg {
    Hidden,
    Metadata,
    SafePreview,
}

impl RichPayloadVisibilityArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Metadata => "metadata",
            Self::SafePreview => "safe_preview",
        }
    }
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
    Policy {
        workspace: String,
        #[arg(long)]
        rich_capture: Option<bool>,
        #[arg(long)]
        image_capture: Option<bool>,
        #[arg(long, value_enum)]
        rich_visibility: Option<RichPayloadVisibilityArg>,
        #[arg(long)]
        agent_raw_payload_access: Option<bool>,
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
    Search {
        workspace: String,
        query: String,
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
    Send {
        workspace: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    Payload {
        #[command(subcommand)]
        command: PayloadCommands,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    Hosted {
        #[command(subcommand)]
        command: HostedCommands,
    },
    AddDemo {
        workspace: String,
        content: String,
        #[arg(long)]
        source_app: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    Recent {
        workspace: String,
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Search {
        workspace: String,
        query: String,
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Bundle {
        workspace: String,
        #[arg(long, default_value_t = DEFAULT_LIST_LIMIT)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum PayloadCommands {
    Inspect {
        payload_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Preview {
        payload_id: String,
        output_path: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Export {
        payload_id: String,
        output_path: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    Plan {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Install {
        #[arg(long)]
        print: bool,
    },
    Uninstall,
    Start,
    Stop,
    Restart,
    Status {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Logs,
}

#[derive(Debug, Subcommand)]
enum HostedCommands {
    Status {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Join {
        #[arg(long)]
        service_url: String,
        #[arg(long)]
        code: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        device_label: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Publish {
        blip_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    StickyShare {
        enabled: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
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
                        let sticky = if workspace.sticky_capture {
                            ", sticky"
                        } else {
                            ""
                        };
                        println!("{} [{access}{sticky}]", workspace.name);
                    }
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &workspaces)?;
                    println!();
                }
            }
        }
        Commands::Policy {
            workspace,
            rich_capture,
            image_capture,
            rich_visibility,
            agent_raw_payload_access,
            output,
        } => {
            let client = DaemonClient::from_config(&config)?;
            let workspaces = client.workspaces()?;
            let current = workspaces
                .workspaces
                .into_iter()
                .find(|candidate| candidate.name == workspace)
                .ok_or_else(|| format!("workspace `{workspace}` does not exist"))?;
            let updated = client.set_workspace_policy(
                &workspace,
                rich_capture.unwrap_or(current.rich_capture_enabled),
                image_capture.unwrap_or(current.image_capture_enabled),
                rich_visibility
                    .map(RichPayloadVisibilityArg::as_str)
                    .unwrap_or(current.rich_payload_visibility.as_str()),
                agent_raw_payload_access.unwrap_or(current.agent_raw_payload_access),
            )?;
            match output {
                OutputFormat::Human => print_workspace_policy(&updated),
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &updated)?;
                    println!();
                }
            }
        }
        Commands::Inbox { limit, output } => {
            let blips = DaemonClient::from_config(&config)?.blips("inbox", limit)?;
            match output {
                OutputFormat::Human => {
                    for blip in blips.blips {
                        println!("{}", format_blip_summary(&blip));
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
            print_blip_list_response(blips, output)?;
        }
        Commands::Search {
            workspace,
            query,
            limit,
            output,
        } => {
            let blips =
                DaemonClient::from_config(&config)?.search_blips(&workspace, &query, limit)?;
            print_blip_list_response(blips, output)?;
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
            DaemonClient::from_config(&config)?.activate_workspace(&workspace)?;
            println!("active workspace set to {workspace}");
        }
        Commands::Send {
            workspace,
            id,
            output,
        } => {
            let client = DaemonClient::from_config(&config)?;
            let routed = match id {
                Some(id) => client.route_blip(&id, &workspace)?,
                None => client.route_latest_inbox_blip(&workspace)?,
            };
            match output {
                OutputFormat::Human => {
                    println!(
                        "routed blip {} from {} to {}",
                        routed.id, routed.from_workspace, routed.to_workspace
                    );
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &routed)?;
                    println!();
                }
            }
        }
        Commands::Agent { command } => match command {
            AgentCommands::Recent {
                workspace,
                limit,
                output,
            } => {
                let blips =
                    DaemonClient::from_config(&config)?.agent_recent_blips(&workspace, limit)?;
                print_agent_blip_list_response(blips, output)?;
            }
            AgentCommands::Search {
                workspace,
                query,
                limit,
                output,
            } => {
                let blips = DaemonClient::from_config(&config)?
                    .agent_search_blips(&workspace, &query, limit)?;
                print_agent_blip_list_response(blips, output)?;
            }
            AgentCommands::Bundle {
                workspace,
                limit,
                output,
            } => {
                let bundle = DaemonClient::from_config(&config)?.agent_bundle(&workspace, limit)?;
                print_agent_bundle_response(bundle, output)?;
            }
        },
        Commands::Payload { command } => match command {
            PayloadCommands::Inspect { payload_id, output } => {
                let payload = DaemonClient::from_config(&config)?.payload_metadata(&payload_id)?;
                print_payload_metadata(&payload, output)?;
            }
            PayloadCommands::Preview {
                payload_id,
                output_path,
                force,
                output,
            } => {
                ensure_export_path_available(&output_path, force)?;
                let payload = DaemonClient::from_config(&config)?
                    .payload_preview(&payload_id, PayloadRequester::Cli)?;
                write_payload_bytes(&output_path, &payload.bytes, force)?;
                print_payload_export_result(&payload, &output_path, output)?;
            }
            PayloadCommands::Export {
                payload_id,
                output_path,
                force,
                output,
            } => {
                ensure_export_path_available(&output_path, force)?;
                let payload = DaemonClient::from_config(&config)?
                    .export_payload(&payload_id, PayloadRequester::Cli)?;
                write_payload_bytes(&output_path, &payload.bytes, force)?;
                print_payload_export_result(&payload, &output_path, output)?;
            }
        },
        Commands::Service { command } => {
            let plan = ServicePlan::from_config(&config)?;
            handle_service_command(command, &config, &plan)?;
        }
        Commands::Hosted { command } => {
            handle_hosted_command(command, &config)?;
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

fn handle_service_command(
    command: ServiceCommands,
    config: &BlipConfig,
    plan: &ServicePlan,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ServiceCommands::Plan { output } => print_service_plan(plan, output),
        ServiceCommands::Install { print } => {
            if print {
                print!("{}", plan.render_install_file()?);
            } else {
                plan.install()?;
                println!("installed blipd service with {}", plan.manager.name());
                if let Some(path) = &plan.service_file_path {
                    println!("service file: {}", path.display());
                }
            }
            Ok(())
        }
        ServiceCommands::Uninstall => {
            plan.uninstall()?;
            println!("uninstalled blipd service");
            Ok(())
        }
        ServiceCommands::Start => {
            plan.start()?;
            println!("started blipd service");
            Ok(())
        }
        ServiceCommands::Stop => {
            plan.stop()?;
            println!("stopped blipd service");
            Ok(())
        }
        ServiceCommands::Restart => {
            plan.restart()?;
            println!("restarted blipd service");
            Ok(())
        }
        ServiceCommands::Status { output } => print_service_status(config, plan, output),
        ServiceCommands::Logs => {
            println!("{}", plan.log_location);
            Ok(())
        }
    }
}

fn handle_hosted_command(
    command: HostedCommands,
    config: &BlipConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = DaemonClient::from_config(config)?;
    match command {
        HostedCommands::Status { output } => {
            let status = client.hosted_status()?;
            print_hosted_status(&status, output)
        }
        HostedCommands::Join {
            service_url,
            code,
            name,
            device_label,
            output,
        } => {
            let status = client.hosted_join_workspace(&service_url, &code, &name, device_label)?;
            print_hosted_status(&status, output)
        }
        HostedCommands::Publish { blip_id, output } => {
            let published = client.hosted_publish_blip(&blip_id)?;
            match output {
                OutputFormat::Human => {
                    println!(
                        "published {} as {} in {} at sequence {}",
                        published.local_blip_id,
                        published.hosted_blip_id,
                        published.hosted_workspace_id,
                        published.sequence
                    );
                }
                OutputFormat::Json => {
                    serde_json::to_writer(io::stdout().lock(), &published)?;
                    println!();
                }
            }
            Ok(())
        }
        HostedCommands::StickyShare { enabled, output } => {
            let enabled = parse_boolish(&enabled)?;
            let status = client.hosted_set_sticky_share(enabled)?;
            print_hosted_status(&status, output)
        }
    }
}

fn parse_boolish(value: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => Err(format!("expected true/false, on/off, yes/no, or 1/0; got `{value}`").into()),
    }
}

fn print_hosted_status(
    status: &blip_api::HostedStatusResponse,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            println!(
                "hosted: {}",
                if status.connected {
                    "connected"
                } else {
                    "not connected"
                }
            );
            println!(
                "service: {}",
                status.service_url.as_deref().unwrap_or("none")
            );
            println!(
                "workspace: {}",
                status.workspace_name.as_deref().unwrap_or("none")
            );
            println!(
                "workspace id: {}",
                status.workspace_id.as_deref().unwrap_or("none")
            );
            println!("member: {}", status.member_id.as_deref().unwrap_or("none"));
            println!("role: {}", status.member_role.as_deref().unwrap_or("none"));
            println!("sticky share: {}", status.sticky_share_enabled);
        }
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout().lock(), status)?;
            println!();
        }
    }
    Ok(())
}

fn print_service_plan(
    plan: &ServicePlan,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            println!("service: blipd");
            println!("manager: {}", plan.manager.name());
            println!("service id: {}", plan.service_id);
            if let Some(path) = &plan.service_file_path {
                println!("service file: {}", path.display());
            } else {
                println!("service file: unsupported");
            }
            println!("blipd: {}", plan.blipd_path.display());
            println!("config: {}", plan.config_path.display());
            println!("database: {}", plan.database_path.display());
            println!("socket: {}", plan.socket_path.display());
            println!("logs: {}", plan.log_location);
        }
        OutputFormat::Json => {
            serde_json::to_writer(
                io::stdout().lock(),
                &render_status_json(plan, false, Some("not checked")),
            )?;
            println!();
        }
    }

    Ok(())
}

fn print_service_status(
    config: &BlipConfig,
    plan: &ServicePlan,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let health_result = DaemonClient::from_config(config)?.health();
    let daemon_running = health_result.is_ok();
    let daemon_error = health_result.as_ref().err().map(ToString::to_string);

    match output {
        OutputFormat::Human => {
            println!("service: blipd");
            println!("manager: {}", plan.manager.name());
            println!(
                "daemon: {}",
                if daemon_running {
                    "running"
                } else {
                    "not running"
                }
            );
            if let Ok(health) = health_result {
                println!("database: {}", health.database_path);
                let active_workspace = health.active_workspace.as_deref().unwrap_or("none");
                println!("active workspace: {active_workspace}");
                println!("generated at: {}", health.generated_at);
            } else if let Some(error) = daemon_error.as_deref() {
                println!("daemon error: {error}");
                println!("start command: blip service start");
            }
            println!("socket: {}", plan.socket_path.display());
            println!("logs: {}", plan.log_location);
        }
        OutputFormat::Json => {
            serde_json::to_writer(
                io::stdout().lock(),
                &render_status_json(plan, daemon_running, daemon_error.as_deref()),
            )?;
            println!();
        }
    }

    Ok(())
}

fn print_agent_blip_list_response(
    response: blip_api::AgentBlipListResponse,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            for blip in response.blips {
                println!(
                    "{} :: {}",
                    blip.id,
                    format_preview(&blip.content, blip.size_bytes)
                );
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout().lock(), &response)?;
            println!();
        }
    }

    Ok(())
}

fn print_agent_bundle_response(
    response: blip_api::AgentBundleResponse,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            print!("{}", response.content);
        }
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout().lock(), &response)?;
            println!();
        }
    }

    Ok(())
}

fn print_workspace_policy(workspace: &WorkspaceSummary) {
    println!("workspace: {}", workspace.name);
    println!("rich capture: {}", workspace.rich_capture_enabled);
    println!("image capture: {}", workspace.image_capture_enabled);
    println!(
        "rich payload visibility: {}",
        workspace.rich_payload_visibility
    );
    println!(
        "agent raw payload access: {}",
        workspace.agent_raw_payload_access
    );
}

fn print_payload_metadata(
    payload: &PayloadSummary,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            println!("payload: {}", payload.id);
            println!("kind: {}", payload.payload_kind);
            println!(
                "mime type: {}",
                payload.mime_type.as_deref().unwrap_or("none")
            );
            println!(
                "platform format: {}",
                payload.platform_format.as_deref().unwrap_or("none")
            );
            println!("byte size: {}", payload.byte_size);
            println!(
                "preview state: {}",
                payload_preview_state(payload.preview_state)
            );
            println!("has blob: {}", payload.has_blob);
            println!("has inline text: {}", payload.has_inline_text);
            if let Some(preview_text) = &payload.preview_text {
                println!("preview text: {preview_text}");
            }
            if let Some(preview_ref) = &payload.preview_ref {
                println!("preview ref: {preview_ref}");
            }
            println!(
                "metadata summary: {}",
                serde_json::to_string(&payload.metadata_summary)?
            );
        }
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout().lock(), payload)?;
            println!();
        }
    }

    Ok(())
}

fn ensure_export_path_available(
    output_path: &Path,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if output_path.try_exists()? && !force {
        return Err(format!(
            "refusing to overwrite existing file: {} (use --force to replace)",
            output_path.display()
        )
        .into());
    }

    let Some(parent) = output_path.parent() else {
        return Ok(());
    };
    if !parent.as_os_str().is_empty() && !parent.try_exists()? {
        return Err(format!("parent directory does not exist: {}", parent.display()).into());
    }

    Ok(())
}

fn write_payload_bytes(
    output_path: &Path,
    bytes: &[u8],
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    std::io::Write::write_all(&mut options.open(output_path)?, bytes)?;
    Ok(())
}

fn print_payload_export_result(
    payload: &blip_api::PayloadBytesResponse,
    output_path: &Path,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            println!(
                "exported payload {} to {} ({} bytes)",
                payload.payload_id,
                output_path.display(),
                payload.byte_size
            );
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "payload_id": payload.payload_id,
                "blip_id": payload.blip_id,
                "workspace": payload.workspace,
                "payload_kind": payload.payload_kind,
                "mime_type": payload.mime_type,
                "path": output_path.display().to_string(),
                "byte_size": payload.byte_size,
            });
            serde_json::to_writer(io::stdout().lock(), &value)?;
            println!();
        }
    }

    Ok(())
}

fn print_blip_list_response(
    response: blip_api::BlipListResponse,
    output: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        OutputFormat::Human => {
            for blip in response.blips {
                println!("{}", format_blip_summary(&blip));
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout().lock(), &response)?;
            println!();
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

fn format_blip_summary(blip: &blip_api::BlipSummary) -> String {
    let flags = blip_summary_flags(blip);
    let preview = format_preview(&blip.preview, blip.size_bytes);
    if flags.is_empty() {
        format!("{} :: {preview}", blip.id)
    } else {
        format!("{} {} :: {preview}", blip.id, flags.join(" "))
    }
}

fn blip_summary_flags(blip: &blip_api::BlipSummary) -> Vec<String> {
    let mut flags = Vec::new();
    if blip.is_redacted {
        flags.push("[redacted]".to_string());
    }
    if let Some(type_tag) = blip
        .tags
        .iter()
        .find(|tag| tag.starts_with(blip_core::TYPE_TAG_PREFIX))
    {
        flags.push(format!("[{type_tag}]"));
    }
    if blip.tags.iter().any(|tag| tag == blip_core::SECRET_TAG) {
        flags.push("[secret]".to_string());
    }
    flags.extend(blip.payloads.iter().filter_map(payload_flag));
    flags
}

fn payload_flag(payload: &blip_api::PayloadSummary) -> Option<String> {
    if payload.payload_kind == "text" {
        return None;
    }

    let mut parts = vec![
        format!("payload:{}", payload.payload_kind),
        payload_preview_state(payload.preview_state).to_owned(),
        format!("id={}", payload.id),
    ];
    if let Some(mime_type) = &payload.mime_type {
        parts.push(format!("mime={mime_type}"));
    }
    if let Some(dimensions) = payload_dimensions(payload) {
        parts.push(format!("dimensions={dimensions}"));
    }
    parts.push(format!("size={}", format_payload_bytes(payload.byte_size)));

    Some(format!("[{}]", parts.join(",")))
}

fn payload_preview_state(state: blip_api::PayloadPreviewState) -> &'static str {
    match state {
        blip_api::PayloadPreviewState::Available => "available",
        blip_api::PayloadPreviewState::TextFallback => "text_fallback",
        blip_api::PayloadPreviewState::MetadataOnly => "metadata_only",
        blip_api::PayloadPreviewState::Redacted => "redacted",
        blip_api::PayloadPreviewState::MissingBlob => "missing_blob",
        blip_api::PayloadPreviewState::Unsupported => "unsupported",
        blip_api::PayloadPreviewState::Unavailable => "unavailable",
    }
}

fn payload_dimensions(payload: &blip_api::PayloadSummary) -> Option<String> {
    let width = payload
        .metadata_summary
        .get("width")
        .and_then(serde_json::Value::as_u64)?;
    let height = payload
        .metadata_summary
        .get("height")
        .and_then(serde_json::Value::as_u64)?;
    Some(format!("{width}x{height}"))
}

fn format_payload_bytes(size_bytes: i64) -> String {
    if size_bytes < 1024 {
        format!("{size_bytes}B")
    } else {
        format!("{:.1}KiB", size_bytes as f64 / 1024.0)
    }
}
