use blip_config::{BlipConfig, config_file_path};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_ID: &str = "com.acozy03.blipcoard.blipd";
const SYSTEMD_UNIT_NAME: &str = "blipd.service";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceManager {
    Launchd,
    SystemdUser,
    UnsupportedWindows,
    Unsupported,
}

impl ServiceManager {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Launchd
        } else if cfg!(target_os = "linux") {
            Self::SystemdUser
        } else if cfg!(target_os = "windows") {
            Self::UnsupportedWindows
        } else {
            Self::Unsupported
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Launchd => "launchd",
            Self::SystemdUser => "systemd --user",
            Self::UnsupportedWindows => "unsupported-windows",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug)]
pub struct ServicePlan {
    pub manager: ServiceManager,
    pub service_id: &'static str,
    pub service_file_path: Option<PathBuf>,
    pub blipd_path: PathBuf,
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub socket_path: PathBuf,
    pub log_location: String,
}

impl ServicePlan {
    pub fn from_config(config: &BlipConfig) -> Result<Self, Box<dyn Error>> {
        let manager = ServiceManager::current();
        let config_path = config_file_path()?;
        let database_path = config.database_path.clone();
        let socket_path = config.daemon_socket_path()?;
        let blipd_path = resolve_blipd_path()?;
        let service_file_path = match manager {
            ServiceManager::Launchd => Some(launch_agent_path()?),
            ServiceManager::SystemdUser => Some(systemd_unit_path()?),
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => None,
        };
        let log_location = match manager {
            ServiceManager::Launchd => launchd_log_path(&database_path).display().to_string(),
            ServiceManager::SystemdUser => {
                "systemd user journal: journalctl --user -u blipd".to_string()
            }
            ServiceManager::UnsupportedWindows => {
                "Windows daemon service startup is blocked until Windows IPC is implemented"
                    .to_string()
            }
            ServiceManager::Unsupported => {
                "foreground stderr/stdout; no platform service integration".to_string()
            }
        };

        Ok(Self {
            manager,
            service_id: SERVICE_ID,
            service_file_path,
            blipd_path,
            config_path,
            database_path,
            socket_path,
            log_location,
        })
    }

    pub fn install(&self) -> Result<(), Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => {
                let path = self
                    .service_file_path
                    .as_ref()
                    .ok_or(ServiceError::UnsupportedManager(self.manager))?;
                if let Some(parent) = launchd_log_path(&self.database_path).parent() {
                    fs::create_dir_all(parent)?;
                }
                write_file(path, &render_launchd_plist(self)?)?;
                Ok(())
            }
            ServiceManager::SystemdUser => {
                let path = self
                    .service_file_path
                    .as_ref()
                    .ok_or(ServiceError::UnsupportedManager(self.manager))?;
                write_file(path, &render_systemd_unit(self))?;
                run_command_allowing_failure(CommandSpec::new(
                    "systemctl",
                    ["--user", "daemon-reload"].map(OsString::from),
                ))?;
                run_command(CommandSpec::new(
                    "systemctl",
                    ["--user", "enable", SYSTEMD_UNIT_NAME].map(OsString::from),
                ))
            }
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }

    pub fn uninstall(&self) -> Result<(), Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => {
                let _ = self.stop();
                if let Some(path) = &self.service_file_path {
                    remove_file_if_exists(path)?;
                }
                Ok(())
            }
            ServiceManager::SystemdUser => {
                let _ = self.stop();
                run_command_allowing_failure(CommandSpec::new(
                    "systemctl",
                    ["--user", "disable", SYSTEMD_UNIT_NAME].map(OsString::from),
                ))?;
                if let Some(path) = &self.service_file_path {
                    remove_file_if_exists(path)?;
                }
                run_command_allowing_failure(CommandSpec::new(
                    "systemctl",
                    ["--user", "daemon-reload"].map(OsString::from),
                ))
            }
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }

    pub fn render_install_file(&self) -> Result<String, Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => render_launchd_plist(self),
            ServiceManager::SystemdUser => Ok(render_systemd_unit(self)),
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }

    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => {
                let path = self
                    .service_file_path
                    .as_ref()
                    .ok_or(ServiceError::UnsupportedManager(self.manager))?;
                run_command(CommandSpec::new(
                    "launchctl",
                    [
                        OsString::from("bootstrap"),
                        OsString::from(format!("gui/{}", current_uid()?)),
                        path.as_os_str().to_owned(),
                    ],
                ))
            }
            ServiceManager::SystemdUser => run_command(CommandSpec::new(
                "systemctl",
                ["--user", "start", SYSTEMD_UNIT_NAME].map(OsString::from),
            )),
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }

    pub fn stop(&self) -> Result<(), Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => run_command(CommandSpec::new(
                "launchctl",
                [
                    OsString::from("bootout"),
                    OsString::from(launchd_service_target(&current_uid()?)),
                ],
            )),
            ServiceManager::SystemdUser => run_command(CommandSpec::new(
                "systemctl",
                ["--user", "stop", SYSTEMD_UNIT_NAME].map(OsString::from),
            )),
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }

    pub fn restart(&self) -> Result<(), Box<dyn Error>> {
        match self.manager {
            ServiceManager::Launchd => {
                let _ = self.stop();
                self.start()
            }
            ServiceManager::SystemdUser => run_command(CommandSpec::new(
                "systemctl",
                ["--user", "restart", SYSTEMD_UNIT_NAME].map(OsString::from),
            )),
            ServiceManager::UnsupportedWindows | ServiceManager::Unsupported => {
                Err(ServiceError::UnsupportedManager(self.manager).into())
            }
        }
    }
}

#[derive(Debug)]
pub enum ServiceError {
    UnsupportedManager(ServiceManager),
    MissingHome,
    CommandFailed {
        program: String,
        status: Option<i32>,
        stderr: String,
    },
}

impl Display for ServiceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedManager(ServiceManager::UnsupportedWindows) => write!(
                formatter,
                "Windows service startup is not available until Windows daemon IPC is implemented"
            ),
            Self::UnsupportedManager(manager) => {
                write!(
                    formatter,
                    "{} service integration is not available",
                    manager.name()
                )
            }
            Self::MissingHome => write!(formatter, "unable to determine the current user home dir"),
            Self::CommandFailed {
                program,
                status,
                stderr,
            } => write!(
                formatter,
                "{program} failed with status {}: {}",
                status
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "terminated by signal".to_string()),
                stderr.trim()
            ),
        }
    }
}

impl Error for ServiceError {}

pub fn render_status_json(
    plan: &ServicePlan,
    daemon_running: bool,
    daemon_error: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "service": "blipd",
        "service_id": plan.service_id,
        "manager": plan.manager.name(),
        "daemon_running": daemon_running,
        "daemon_error": daemon_error,
        "service_file": plan.service_file_path.as_ref().map(|path| path.display().to_string()),
        "blipd_path": plan.blipd_path.display().to_string(),
        "config_path": plan.config_path.display().to_string(),
        "database_path": plan.database_path.display().to_string(),
        "socket_path": plan.socket_path.display().to_string(),
        "logs": plan.log_location,
    })
}

fn resolve_blipd_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = env::var_os("BLIPCOARD_BLIPD_PATH") {
        return Ok(PathBuf::from(path));
    }

    let current_exe = env::current_exe()?;
    let Some(parent) = current_exe.parent() else {
        return Ok(PathBuf::from("blipd"));
    };
    Ok(parent.join(exe_name("blipd")))
}

fn exe_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn systemd_unit_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(home_dir()?
        .join(".config/systemd/user")
        .join(SYSTEMD_UNIT_NAME))
}

fn launch_agent_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(home_dir()?
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_ID}.plist")))
}

fn launchd_log_path(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs/blipd.log")
}

fn launchd_service_target(uid: &str) -> String {
    format!("gui/{uid}/{SERVICE_ID}")
}

fn home_dir() -> Result<PathBuf, ServiceError> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ServiceError::MissingHome)
}

fn write_file(path: &Path, contents: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn render_systemd_unit(plan: &ServicePlan) -> String {
    format!(
        "[Unit]\n\
         Description=blipcoard daemon\n\
         Documentation=https://github.com/blipcoard/blipcoard\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         Environment=BLIPCOARD_CONFIG_DIR={}\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        shell_escape(&plan.blipd_path),
        shell_escape(plan.config_path.parent().unwrap_or_else(|| Path::new(".")))
    )
}

fn render_launchd_plist(plan: &ServicePlan) -> Result<String, Box<dyn Error>> {
    let log_path = launchd_log_path(&plan.database_path);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
           <key>Label</key>\n\
           <string>{}</string>\n\
           <key>ProgramArguments</key>\n\
           <array>\n\
             <string>{}</string>\n\
           </array>\n\
           <key>EnvironmentVariables</key>\n\
           <dict>\n\
             <key>BLIPCOARD_CONFIG_DIR</key>\n\
             <string>{}</string>\n\
           </dict>\n\
           <key>RunAtLoad</key>\n\
           <true/>\n\
           <key>KeepAlive</key>\n\
           <true/>\n\
           <key>StandardOutPath</key>\n\
           <string>{}</string>\n\
           <key>StandardErrorPath</key>\n\
           <string>{}</string>\n\
         </dict>\n\
         </plist>\n",
        SERVICE_ID,
        xml_escape(&plan.blipd_path.display().to_string()),
        xml_escape(
            &plan
                .config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .display()
                .to_string()
        ),
        xml_escape(&log_path.display().to_string()),
        xml_escape(&log_path.display().to_string())
    ))
}

fn shell_escape(path: &Path) -> String {
    let value = path.display().to_string();
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._:-".contains(character))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

impl CommandSpec {
    fn new<I>(program: impl Into<OsString>, args: I) -> Self
    where
        I: IntoIterator<Item = OsString>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
        }
    }
}

fn run_command(spec: CommandSpec) -> Result<(), Box<dyn Error>> {
    let output = Command::new(&spec.program).args(&spec.args).output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(ServiceError::CommandFailed {
        program: spec.program.to_string_lossy().into_owned(),
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
    .into())
}

fn run_command_allowing_failure(spec: CommandSpec) -> Result<(), Box<dyn Error>> {
    match run_command(spec) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("{error}");
            Ok(())
        }
    }
}

fn current_uid() -> Result<String, Box<dyn Error>> {
    let output = Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        return Err(ServiceError::CommandFailed {
            program: "id".to_string(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plan(manager: ServiceManager) -> ServicePlan {
        ServicePlan {
            manager,
            service_id: SERVICE_ID,
            service_file_path: Some(PathBuf::from(
                "/home/user/.config/systemd/user/blipd.service",
            )),
            blipd_path: PathBuf::from("/opt/blipcoard/bin/blipd"),
            config_path: PathBuf::from("/home/user/.config/blipcoard/config.toml"),
            database_path: PathBuf::from("/home/user/.local/share/blipcoard/blipcoard.db"),
            socket_path: PathBuf::from("/home/user/.local/share/blipcoard/blipcoard.sock"),
            log_location: "journal".to_string(),
        }
    }

    #[test]
    fn systemd_unit_runs_blipd_as_user_service() {
        let unit = render_systemd_unit(&test_plan(ServiceManager::SystemdUser));

        assert!(unit.contains("ExecStart=/opt/blipcoard/bin/blipd"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("BLIPCOARD_CONFIG_DIR=/home/user/.config/blipcoard"));
    }

    #[test]
    fn launchd_plist_runs_at_login_and_keeps_daemon_alive() {
        let plist =
            render_launchd_plist(&test_plan(ServiceManager::Launchd)).expect("plist should render");

        assert!(plist.contains("<string>com.acozy03.blipcoard.blipd</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("/opt/blipcoard/bin/blipd"));
        assert!(plist.contains("logs/blipd.log"));
    }

    #[test]
    fn launchd_stop_uses_full_service_target() {
        assert_eq!(
            launchd_service_target("501"),
            "gui/501/com.acozy03.blipcoard.blipd"
        );
    }

    #[test]
    fn status_json_includes_paths_and_daemon_state() {
        let plan = test_plan(ServiceManager::SystemdUser);
        let status = render_status_json(&plan, false, Some("daemon is not running"));

        assert_eq!(status["service"], "blipd");
        assert_eq!(status["manager"], "systemd --user");
        assert_eq!(status["daemon_running"], false);
        assert_eq!(status["daemon_error"], "daemon is not running");
        assert_eq!(
            status["socket_path"],
            "/home/user/.local/share/blipcoard/blipcoard.sock"
        );
    }
}
