use std::error::Error;

use bluest::Adapter;
use tokio_stream::StreamExt;
use tracing::info;
use tracing::metadata::LevelFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let adapter = Adapter::default().await.unwrap();
    adapter.wait_available().await?;

    info!("starting scan");
    let mut scan = adapter.scan(&[]).await?;
    info!("scan started");
    while let Some(discovered_device) = scan.next().await {
        if discovered_device.adv_data.local_name.is_some() {
            info!(
                "{} ({}dBm): {:?}",
                discovered_device.adv_data.local_name.as_ref().unwrap(),
                discovered_device.rssi.unwrap(),
                discovered_device.adv_data.services
            );
        }
    }

    Ok(())
}
