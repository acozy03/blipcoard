use blip_clipboard::{ClipboardWatcherConfig, system_watcher};
use blip_config::BlipConfig;
use blip_core::BlipStore;
use blip_daemon::{
    ClipboardIngestionSource, DaemonRuntime, PendingIngestionSource, ipc::DaemonIpcServer,
};
use std::thread;

const IPC_ONLY_ARG: &str = "--ipc-only";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BlipConfig::load_or_create()?;
    if std::env::args().any(|arg| arg == IPC_ONLY_ARG) {
        return serve_ipc(&config);
    }

    start_ipc_server(&config)?;

    let store = BlipStore::open(&config.database_path)?;
    let clipboard_config = ClipboardWatcherConfig::default();
    let idle_interval = clipboard_config.poll_interval;
    let watcher = system_watcher(clipboard_config)?;
    let mut runtime = DaemonRuntime::new(
        &config.database_path,
        store,
        ClipboardIngestionSource::new(watcher, idle_interval),
    );
    let response = runtime.health_response()?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    runtime.run()?;

    Ok(())
}

fn start_ipc_server(config: &BlipConfig) -> Result<(), Box<dyn std::error::Error>> {
    let database_path = config.database_path.clone();
    let socket_path = config.daemon_socket_path()?;
    let store = BlipStore::open(&database_path)?;
    let mut runtime = DaemonRuntime::new(&database_path, store, PendingIngestionSource::default());
    let ipc_server = DaemonIpcServer::new(&socket_path).bind()?;

    thread::spawn(move || {
        if let Err(error) = ipc_server.serve(|request| runtime.dispatch_daemon_request(request)) {
            eprintln!("daemon IPC server stopped: {error}");
        }
    });

    Ok(())
}

fn serve_ipc(config: &BlipConfig) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = config.daemon_socket_path()?;
    let store = BlipStore::open(&config.database_path)?;
    let mut runtime = DaemonRuntime::new(
        &config.database_path,
        store,
        PendingIngestionSource::default(),
    );

    eprintln!("serving daemon IPC on {}", socket_path.display());
    DaemonIpcServer::new(socket_path).serve(|request| runtime.dispatch_daemon_request(request))?;

    Ok(())
}
