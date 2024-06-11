// This is designed to be used in conjunction with the l2cap_server example from bluer. https://github.com/bluez/bluer/blob/869dab889140e3be5a0f1791c40825730893c8b6/bluer/examples/l2cap_server.rs

use std::error::Error;

use bluest::{Adapter, Uuid as BluestUUID};
use futures_lite::StreamExt;
use tokio::io::AsyncReadExt;
use tracing::info;
use tracing::metadata::LevelFilter;

#[cfg(target_os = "linux")]
const SERVICE_UUID: BluestUUID = bluer::Uuid::from_u128(0xFEED0000F00D);

#[cfg(not(target_os = "linux"))]
use uuid::Uuid;
#[cfg(not(target_os = "linux"))]
const SERVICE_UUID: BluestUUID = Uuid::from_u128(0xFEED0000F00D);

const PSM: u16 = 0x80 + 5;

const HELLO_MSG: &[u8] = b"Hello from l2cap_server!";

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

    info!("looking for device");
    let device = adapter
        .discover_devices(&[SERVICE_UUID])
        .await?
        .next()
        .await
        .ok_or("Failed to discover device")??;
    info!(
        "found device: {} ({:?})",
        device.name().as_deref().unwrap_or("(unknown)"),
        device.id()
    );

    adapter.connect_device(&device).await.unwrap();

    let mut channel = device.open_l2cap_channel(PSM, true).await.unwrap();

    info!("Reading from channel.");
    let mut hello_buf = [0u8; HELLO_MSG.len()];
    channel.read_exact(&mut hello_buf).await.unwrap();

    info!("Got {} from channel", std::str::from_utf8(&hello_buf).unwrap());
    assert_eq!(hello_buf, HELLO_MSG);
    Ok(())
}
