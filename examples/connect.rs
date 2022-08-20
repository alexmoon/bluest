use std::{error::Error, time::Duration};

use bluest::{Adapter, BluetoothUuidExt};
use tokio_stream::StreamExt;
use tracing::{info, metadata::LevelFilter};
use uuid::Uuid;

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

    let adapter = Adapter::default().await.unwrap();
    adapter.wait_available().await?;

    let discovered_device = {
        info!("starting scan");
        let services = &[Uuid::from_u16(0x181c)];
        let mut scan = adapter.scan(services).await?;
        info!("scan started");
        scan.next().await.unwrap() // this will never timeout
    };

    info!("{:?} {:?}", discovered_device.rssi, discovered_device.adv_data);
    adapter.connect_device(&discovered_device.device).await?; // this will never timeout
    info!("connected!");

    tokio::time::sleep(Duration::from_secs(30)).await;

    adapter.disconnect_device(&discovered_device.device).await?;
    info!("disconnected!");

    Ok(())
}
