use blip_cloud::{CloudConfig, CloudStore, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CloudConfig::from_env();
    let store = CloudStore::open(&config.database_path)?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;

    println!("blip-cloud listening on http://{}", config.bind_addr);
    axum::serve(listener, app(store)).await?;
    Ok(())
}
