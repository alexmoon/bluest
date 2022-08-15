use std::error::Error;

use bluest::{BluetoothUuidExt, Session};
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

    let session = Session::new().await?;
    let adapter = session.default_adapter().await.unwrap();
    adapter.wait_available().await;

    let discovered_device = {
        info!("starting scan");
        let mut scan = adapter.scan(Some(&[Uuid::from_u16(0x181c)])).await?;
        info!("scan started");
        scan.next().await.unwrap() // this will never timeout
    };

    info!("{} {:?}", discovered_device.rssi, discovered_device.adv_data);
    adapter.connect(&discovered_device.device).await?; // this will never timeout
    info!("connected!");
    adapter.disconnect(&discovered_device.device).await?;
    info!("disconnected!");

    Ok(())
}
