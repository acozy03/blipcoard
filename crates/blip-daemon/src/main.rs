use blip_clipboard::{ClipboardWatcherConfig, system_watcher};
use blip_config::BlipConfig;
use blip_core::BlipStore;
use blip_daemon::{ClipboardIngestionSource, DaemonRuntime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BlipConfig::load_or_create()?;
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
