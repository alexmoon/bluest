#[cfg(target_os = "linux")]
use bluer::{Session, adv::{Advertisement, AdvertisementHandle, Type}};
use std::{collections::BTreeMap, time::Duration};

use crate::{AdvertisementData, AdvertisingGuard};

use super::adapter::AdapterImpl;

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct AdvertisementImpl {
    advertisement_handle: Option<AdvertisementHandle>,
}

impl AdvertisementImpl {
    /// Creates a new `PlatformAdvertisementImpl` instance with the specified adapter.
    pub fn new() -> Self {
        Self {
            advertisement_handle: None,
        }
    }

    // /// Start advertising on Linux using `bluer`.
    // pub async fn advertise(&mut self, data: &Vec<u8>, advertise_duration: Option<Duration>) -> bluer::Result<()> {
    //     // Stop any existing advertisement
    //     self.stop_advertising()?;

    //     // Configure the advertisement
    //     let le_advertisement = Advertisement {
    //         advertisement_type: Type::Peripheral,
    //         service_uuids: vec![]
    //             .into_iter()
    //             .collect(),
    //         local_name: None,
    //         discoverable: Some(true),
    //         manufacturer_data: data.manufacturer_data.map(|m| {
    //             let mut map = BTreeMap::new();
    //             map.insert(m.company_id, m.data);
    //             map}),
    //         ..Default::default()
    //     };

    //     // Start advertising
    //     let handle = self.adapter.advertise(le_advertisement).await?;
    //     self.advertisement_handle = Some(handle);

    //     if let Some(duration) = advertise_duration {
    //         sleep(duration).await;
    //         self.stop_advertising()?; // Stop the advertisement after the duration
    //         println!("Linux advertisement stopped after {:?}", duration);
    //     }

    //     Ok(())
    // }

    /// Stop advertising if an advertisement is active
    pub fn stop_advertising(&mut self) -> bluer::Result<()> {
        if let Some(handle) = self.advertisement_handle.take() {
            println!("Linux advertisement manually stopped.");
            drop(handle); // Dropping the handle stops the advertisement
        }
        Ok(())
    }

    /// Start advertising and return an AdvertisingGuard
    pub async fn start_advertising(mut self, data: AdvertisementData) -> Result<AdvertisingGuard, String> {
        println!("START ADVERTISOMG ***");
    // Convert manufacturer_data to the expected BTreeMap format
        let manufacturer_data: BTreeMap<u16, Vec<u8>> = data.manufacturer_data
        .map(|manufacturer_data| {
            let mut map = BTreeMap::new();
            map.insert(manufacturer_data.company_id, manufacturer_data.data.clone());
            map
        })
        .unwrap_or_default();

        let le_advertisement = Advertisement {
            advertisement_type: Type::Broadcast,
            service_uuids: vec![]
                .into_iter()
                .collect(),
            local_name: Some("le_advertise".to_string()),
            discoverable: Some(true),
            manufacturer_data: manufacturer_data,
            ..Default::default()
        };
        let adapter = AdapterImpl::default().await;
        match adapter {
            Some(adapter) => {
                let handle = adapter.inner.advertise(le_advertisement).await.map_err(|e| format!("Failed to start advertising: {:?}", e))?;
                self.advertisement_handle = Some(handle);        
            },
            None=>{}
        }
        
        Ok(AdvertisingGuard { advertisement: self })
    }
}

/// Struct to handle advertisement cleanup on drop for Linux
#[cfg(target_os = "linux")]
impl Drop for AdvertisementImpl {
    fn drop(&mut self) {
        let _ = self.stop_advertising();
    }
}