use blip_api::HealthResponse;
use blip_config::BlipConfig;
use blip_core::BlipStore;
use chrono::Utc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BlipConfig::load_or_create()?;
    let store = BlipStore::open(&config.database_path)?;
    let active_workspace = store.get_active_workspace()?;

    let response = HealthResponse {
        service: "blipd".to_string(),
        status: "ready".to_string(),
        database_path: config.database_path.display().to_string(),
        active_workspace,
        generated_at: Utc::now(),
    };

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
