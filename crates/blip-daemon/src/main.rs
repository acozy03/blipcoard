use blip_config::BlipConfig;
use blip_core::BlipStore;
use blip_daemon::{DaemonRuntime, PendingIngestionSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BlipConfig::load_or_create()?;
    let store = BlipStore::open(&config.database_path)?;
    let mut runtime = DaemonRuntime::new(
        &config.database_path,
        store,
        PendingIngestionSource::default(),
    );
    let response = runtime.health_response()?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    runtime.run()?;

    Ok(())
}
