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


/// A Bluetooth Advertisement
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

    /// Stops the advertisement.
    pub fn stop_advertising(&mut self) -> Result<(), io::Error> {
        self.inner.stop_advertising()
    }
}

#[derive(Debug, Clone)]
pub struct AdvertisingGuard {
    pub(crate) advertisement: Advertisement,
}

impl Drop for AdvertisingGuard {
    fn drop(&mut self) {
        // Stop advertising when `AdvertisingGuard` is dropped.
        if let Err(e) = self.advertisement.stop_advertising() {
            eprintln!("Warning: Failed to stop advertising: {:?}", e);
        }
    }
}