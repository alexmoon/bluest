use std::error::Error;

use bluest::{btuuid, Adapter};
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

    let adapter = Adapter::default().await.unwrap();
    adapter.wait_available().await?;

    info!("getting connected devices");
    // let devices = adapter.connected_devices().await?;
    let devices = adapter
        .connected_devices_with_services(&[btuuid::services::BATTERY])
        .await?;
    for device in devices {
        info!("found {:?}", device);
        adapter.connect_device(&device).await?;
        let services = device.discover_services().await?;
        for service in services {
            info!("  {:?}", service);
            let characteristics = service.discover_characteristics().await?;
            for characteristic in characteristics {
                info!("    {:?}", characteristic);
                let descriptors = characteristic.discover_descriptors().await?;
                for descriptor in descriptors {
                    info!("      {:?}", descriptor);
                }
            }
        }
        adapter.disconnect_device(&device).await?;
    }
    info!("done");

    Ok(())
}
