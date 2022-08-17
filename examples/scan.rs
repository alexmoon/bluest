use std::error::Error;

use bluest::Session;
use tokio_stream::StreamExt;
use tracing::{info, metadata::LevelFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let session = Session::new().await?;
    let adapter = session.default_adapter().await.unwrap();
    adapter.wait_available().await?;

    info!("starting scan");
    let mut scan = adapter.scan(None).await?;
    info!("scan started");
    while let Some(discovered_device) = scan.next().await {
        info!("{} {:?}", discovered_device.rssi, discovered_device.adv_data);
    }

    Ok(())
}
