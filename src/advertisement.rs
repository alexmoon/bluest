use std::time::Duration;
use std::io; // Use std::io::Error for simplicity

use crate::windows::adapter::AdapterImpl;
#[cfg(target_os = "windows")]
use crate::windows_advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(target_os = "android")]
use crate::android::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::corebluetooth::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;

#[cfg(target_os = "linux")]
use crate::bluer::advertisement::AdvertisementImpl as PlatformAdvertisementImpl;
use crate::AdvertisementData;

#[derive(Debug, Clone)]
pub struct Advertisement {
    inner: PlatformAdvertisementImpl,
}

impl Advertisement {
    /// Creates a new `Advertisement` instance with the specified company ID.
    pub fn new() -> Self {
        Self {
            inner: PlatformAdvertisementImpl::new(),
        }
    }

    /// Starts advertising for the specified duration.
    pub async fn advertise(&mut self, data: &Vec<u8>, advertise_duration: Option<Duration>) -> Result<(), io::Error> {
        self.inner.advertise(data, advertise_duration).await
    }

    /// Stops the advertisement.
    pub fn stop(&mut self) -> Result<(), io::Error> {
        self.inner.stop()
        //Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AdvertisingGuard {
    pub(crate) advertisement: Advertisement,
}

impl Drop for AdvertisingGuard {
    fn drop(&mut self) {
        // Stop advertising when `AdvertisingGuard` is dropped.
        self.advertisement.stop().expect("Failed to stop advertising");
    }
}