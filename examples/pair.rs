use std::error::Error;

use async_trait::async_trait;
use bluest::pairing::{IoCapability, PairingAgent, PairingRejected, Passkey};
use bluest::{btuuid, Adapter, DeviceId};
use futures_util::StreamExt;
use tracing::info;
use tracing::metadata::LevelFilter;

struct StdioPairingAgent;

#[async_trait]
impl PairingAgent for StdioPairingAgent {
    /// The input/output capabilities of this agent
    fn io_capability(&self) -> IoCapability {
        IoCapability::KeyboardDisplay
    }

    async fn confirm(&self, id: &DeviceId) -> Result<(), PairingRejected> {
        tokio::task::block_in_place(move || {
            println!("Do you want to pair with {:?}? (Y/n)", id);
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .map_err(|_| PairingRejected::default())?;
            if buf.is_empty() || buf == "y" || buf == "Y" {
                Ok(())
            } else {
                Err(PairingRejected::default())
            }
        })
    }

    async fn confirm_passkey(&self, id: &DeviceId, passkey: Passkey) -> Result<(), PairingRejected> {
        tokio::task::block_in_place(move || {
            println!("Is the passkey \"{}\" displayed on {:?}? (Y/n)", passkey, id);
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .map_err(|_| PairingRejected::default())?;
            if buf.is_empty() || buf == "y" || buf == "Y" {
                Ok(())
            } else {
                Err(PairingRejected::default())
            }
        })
    }

    async fn request_passkey(&self, id: &DeviceId) -> Result<Passkey, PairingRejected> {
        tokio::task::block_in_place(move || {
            println!("Please enter a 6-digit passkey for {:?}: ", id);
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .map_err(|_| PairingRejected::default())?;
            buf.parse().map_err(|_| PairingRejected::default())
        })
    }

    fn display_passkey(&self, id: &DeviceId, passkey: Passkey) {
        println!("The passkey is \"{}\" for {:?}.", passkey, id);
    }
}

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

    let adapter = Adapter::default().await.ok_or("Bluetooth adapter not found")?;
    adapter.wait_available().await?;

    let discovered_device = {
        info!("starting scan");
        let mut scan = adapter.scan(&[btuuid::services::HUMAN_INTERFACE_DEVICE]).await?;
        info!("scan started");
        scan.next().await.ok_or("scan terminated")?
    };

    info!("{:?} {:?}", discovered_device.rssi, discovered_device.adv_data);
    let device = discovered_device.device;

    adapter.connect_device(&device).await?;
    info!("connected!");

    device.pair_with_agent(&StdioPairingAgent).await?;
    info!("paired!");

    Ok(())
}
