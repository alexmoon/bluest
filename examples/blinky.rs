use std::error::Error;
use std::time::Duration;

use bluest::{Adapter, Uuid};
use futures_util::future::{select, Either};
use futures_util::{pin_mut, StreamExt};
use tracing::metadata::LevelFilter;
use tracing::{error, info};

const NORDIC_LED_AND_BUTTON_SERVICE: Uuid = Uuid::from_u128(0x00001523_1212_efde_1523_785feabcd123);
const BLINKY_BUTTON_STATE_CHARACTERISTIC: Uuid = Uuid::from_u128(0x00001524_1212_efde_1523_785feabcd123);
const BLINKY_LED_STATE_CHARACTERISTIC: Uuid = Uuid::from_u128(0x00001525_1212_efde_1523_785feabcd123);

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

    info!("looking for device");
    let device = adapter
        .discover_devices(&[NORDIC_LED_AND_BUTTON_SERVICE])
        .await?
        .next()
        .await
        .ok_or("Failed to discover device")??;
    info!(
        "found device: {} ({:?})",
        device.name().as_deref().unwrap_or("(unknown)"),
        device.id()
    );

    adapter.connect_device(&device).await?; // this will never timeout
    info!("connected!");

    let service = match device
        .discover_services_with_uuid(NORDIC_LED_AND_BUTTON_SERVICE)
        .await?
        .get(0)
    {
        Some(service) => service.clone(),
        None => return Err("service not found".into()),
    };
    info!("found LED and button service");

    let characteristics = service.discover_characteristics().await?;
    info!("discovered characteristics");

    let button_characteristic = characteristics
        .iter()
        .find(|x| x.uuid() == BLINKY_BUTTON_STATE_CHARACTERISTIC)
        .ok_or("button characteristic not found")?;

    let button_fut = async {
        info!("enabling button notifications");
        let updates = button_characteristic.notify().await?;
        pin_mut!(updates);
        info!("waiting for button changes");
        while let Some(val) = updates.next().await {
            info!("Button state changed: {:?}", val?);
        }
        Ok(())
    };
    pin_mut!(button_fut);

    let led_characteristic = characteristics
        .iter()
        .find(|x| x.uuid() == BLINKY_LED_STATE_CHARACTERISTIC)
        .ok_or("led characteristic not found")?;

    let blink_fut = async {
        info!("blinking LED");
        tokio::time::sleep(Duration::from_secs(1)).await;
        loop {
            led_characteristic.write(&[0x01]).await?;
            info!("LED on");
            tokio::time::sleep(Duration::from_secs(1)).await;
            led_characteristic.write(&[0x00]).await?;
            info!("LED off");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    pin_mut!(blink_fut);

    type R = Result<(), Box<dyn Error>>;
    let res: Either<(R, _), (R, _)> = select(blink_fut, button_fut).await;
    match res {
        Either::Left((res, button_fut)) => {
            error!("Blink task exited: {:?}", res);
            let res = button_fut.await;
            error!("Button task exited: {:?}", res);
        }
        Either::Right((res, blink_fut)) => {
            error!("Button task exited: {:?}", res);
            let res = blink_fut.await;
            error!("Blink task exited: {:?}", res);
        }
    }

    Ok(())
}
