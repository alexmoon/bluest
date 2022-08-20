use std::{error::Error, time::Duration};

use bluest::Adapter;
use futures::future::Either;
use tokio_stream::StreamExt;
use tracing::{error, info, metadata::LevelFilter};
use uuid::Uuid;

const NORDIC_LED_AND_BUTTON_SERVICE: Uuid = Uuid::from_u128(0x00001523_1212_efde_1523_785feabcd123);
const BLINKY_BUTTON_STATE_CHARACTERISTIC: Uuid = Uuid::from_u128(0x00001524_1212_efde_1523_785feabcd123);
const BLINKY_LED_STATE_CHARACTERISTIC: Uuid = Uuid::from_u128(0x00001525_1212_efde_1523_785feabcd123);

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
        let services = &[NORDIC_LED_AND_BUTTON_SERVICE];
        let mut scan = adapter.scan(services).await?;
        info!("scan started");
        scan.next().await.unwrap() // this will never timeout
    };

    info!(
        "found device: {:?}dBm {:?}",
        discovered_device.rssi.unwrap(),
        discovered_device.adv_data.services
    );
    let device = discovered_device.device;
    adapter.connect_device(&device).await?; // this will never timeout
    info!("connected!");

    loop {
        let services_changed = Box::pin(device.services_changed());
        let task = async {
            let service = match device
                .discover_services(Some(NORDIC_LED_AND_BUTTON_SERVICE))
                .await?
                .get(0)
                .cloned()
            {
                Some(service) => service,
                None => return Err("service not found".into()),
            };
            info!("found LED and button service");

            let characteristics = service.discover_characteristics(None).await?;
            info!("discovered characteristics");

            let button_characteristic = characteristics
                .iter()
                .find(|x| x.uuid() == BLINKY_BUTTON_STATE_CHARACTERISTIC)
                .unwrap()
                .clone();

            let led_characteristic = characteristics
                .iter()
                .find(|x| x.uuid() == BLINKY_LED_STATE_CHARACTERISTIC)
                .unwrap();

            let blink_fut = Box::pin(async {
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
            });

            let button_fut = Box::pin(async {
                info!("enabling button notifications");
                let mut updates = button_characteristic.notify().await?;
                info!("waiting for button changes");
                while let Some(val) = updates.next().await {
                    info!("Button state changed: {:?}", val?);
                }
                Ok(())
            });

            type R = Result<(), Box<dyn Error>>;
            let res: Either<(R, _), (R, _)> = futures::future::select(blink_fut, button_fut).await;
            match res {
                futures::future::Either::Left((res, button_fut)) => {
                    error!("Blink task exited: {:?}", res);
                    button_fut.await
                }
                futures::future::Either::Right((res, blink_fut)) => {
                    error!("Button task exited: {:?}", res);
                    blink_fut.await
                }
            }
        };

        let task = Box::pin(task);
        match futures::future::select(task, services_changed).await {
            futures::future::Either::Left((res, _)) => res?,
            futures::future::Either::Right(_) => (),
        }
    }
}
