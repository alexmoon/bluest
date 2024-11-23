use tracing::debug;

use std::convert::Infallible;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::io; use crate::bluer::adapter::AdapterImpl;
// Use std::io::Error for simplicity
use crate::{Adapter, AdvertisementData, AdvertisingGuard};

#[cfg(target_os = "windows")]
use crate::windows::adapter::AdapterImpl;

#[cfg(target_os = "windows")]
use crate::windows_advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(target_os = "android")]
use crate::android::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::corebluetooth::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(target_os = "linux")]
use crate::bluer::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;


// /// A Bluetooth Advertisement
// #[derive(Debug)]
// pub struct Advertisement {
//     inner: PlatformAdvertisementImpl,
// }

// impl Advertisement {
//     /// Creates a new `Advertisement` instance with the specified company ID.
//     pub fn new(adapter: AdapterImpl) -> Self {
//         Self {
//             inner: PlatformAdvertisementImpl::new(adapter),
//         }
//     }

//     /// Stops the advertisement.
//     pub fn stop_advertising(&mut self) -> Result<(), bluer::Error> {
//         self.inner.stop_advertising()
//     }

//     pub async fn start_advertising(&mut self, data: AdvertisementData) -> Result<AdvertisingGuard, String> {
//         self.inner.start_advertising(data).await
//     }
// }


#[derive(Debug)]
pub struct Advertisement {
    inner: PlatformAdvertisementImpl,
}

impl Advertisement {
    /// Creates a new `Advertisement` instance with the specified adapter.
    pub fn new() -> Self {
        Self {
            inner: PlatformAdvertisementImpl::new(),
        }
    }

    /// Starts advertising with the given data.
    pub async fn start_advertising(mut self, data: AdvertisementData) -> Result<AdvertisingGuard, String> {
        self.inner.start_advertising(data).await
    }

    /// Stops the advertisement.
    pub fn stop_advertising(mut self) -> Result<(),bluer::Error> {
        self.inner.stop_advertising()
    }
}

