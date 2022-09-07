#![warn(missing_docs)]

//! Bluest is a cross-platform [Bluetooth Low Energy] (BLE) library for [Rust]. It currently supports Windows (version
//! 10 and later), MacOS/iOS, and Linux. Android support is planned.
//!
//! The goal of Bluest is to create a *thin* abstraction on top of the platform-specific Bluetooth APIs in order to
//! provide safe, cross-platform access to Bluetooth LE devices. The crate currently supports the GAP Central and
//! GATT Client roles. Peripheral and Server roles are not supported.
//!
//! [Rust]: https://www.rust-lang.org/
//! [Bluetooth Low Energy]: https://www.bluetooth.com/specifications/specs/
//!
//! # Usage
//!
//! ```rust,no_run
//!# use bluest::Adapter;
//!# use futures_util::StreamExt;
//!# #[tokio::main]
//!# async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!let adapter = Adapter::default().await.ok_or("Bluetooth adapter not found")?;
//!adapter.wait_available().await?;
//!
//!println!("starting scan");
//!let mut scan = adapter.scan(&[]).await?;
//!println!("scan started");
//!while let Some(discovered_device) = scan.next().await {
//!    println!(
//!        "{}{}: {:?}",
//!        discovered_device.device.name().as_deref().unwrap_or("(unknown)"),
//!        discovered_device
//!            .rssi
//!            .map(|x| format!(" ({}dBm)", x))
//!            .unwrap_or_default(),
//!        discovered_device.adv_data.services
//!    );
//!}
//!#
//!#    Ok(())
//!# }
//! ```
//!
//! # Overview
//!
//! The primary functions provided by Bluest are:
//!
//! - Device discovery:
//!   - [Scanning][Adapter::scan] for devices and receiving advertisements
//!   - Finding [connected devices][Adapter::connected_devices]
//!   - [Opening][Adapter::open_device] previously found devices
//!   - [Connecting][Adapter::connect_device] to discovered devices
//! - Accessing remote GATT services:
//!   - Discovering device [services][Device::discover_services]
//!   - Discovering service [characteristics][Service::discover_characteristics]
//!   - Discovering characteristic [descriptors][Characteristic::discover_descriptors]
//!   - [Read][Characteristic::read], [write][Characteristic::write] (including
//!     [write without response][Characteristic::write_without_response]), and
//!     [notify/indicate][Characteristic::notify] operations on remote characteristics
//!   - [Read][Descriptor::read] and [write][Descriptor::write] operations on characteristic descriptors
//!
//! # Asynchronous runtimes
//!
//! On non-linux platforms, Bluest should work with any asynchronous runtime. On linux the underlying `bluer` crate
//! requires the Tokio runtime and Bluest makes use of Tokio's `block_in_place` API (which requires Tokio's
//! multi-threaded runtime) to make a few methods synchronous. Linux-only asynchronous versions of those methods are
//! also provided, which should be preferred in platform-specific code.
//!
//! # Platform specifics
//!
//! Because Bluest aims to provide a thin abstraction over the platform-specific APIs, the available APIs represent the
//! lowest common denominator of APIs among the supported platforms. In most cases Apple's CoreBluetooth API is the
//! most restricted and therefore imposes the limit on what can be supported in a cross platform library. For example,
//! CoreBluetooth never exposes the Bluetooth address of devices to applications, therefore there is no method on
//! `Device` for retrieving an address or even any Bluetooth address struct in the crate.
//!
//! The underlying APIs for accessing services, characteristics, and descriptors are all pretty similar and should
//! behave consistently across platforms. However errors may not be consistent from platform to platform. For example,
//! Linux's bluez API does not return the underlying Bluetooth protocol error in a useful way, whereas the other
//! platforms do. Where it is possible to return a meaningful error, Bluest will attempt to do so. In other cases,
//! Bluest may return an error with a [`kind`][Error::kind] of [`Other`][error::ErrorKind::Other] and you would need to
//! look at the platform-specific [`source`][std::error::Error::source] of the error for more information.
//!
//! The more significant area of platform differences is in discovering and connecting to devices.
//!
//! Each platform has its own methods for identifying, scanning for, and connecting to devices. Again, since
//! CoreBluetooth is the most restrictive in the methods it provides for filtering scan results and identifying devices
//! to connect to, the Bluest API largely follows those limitations (e.g. scanning can be filtered only by a set of
//! service UUIDs). The implementations for other platforms have been poly-filled to match those APIs.
//!
//! Connecting and disconnecting from devices is another area of API differences that cannot be as easily poly-filled.
//! To ensure proper cross-platform behavior, you should always call [`connect_device`][Adapter::connect_device] before
//! calling any methods which may require a connection. When you have finished using a device you should call
//! [`disconnect_device`][Adapter::disconnect_device] and then drop the `Device` and all its child objects to ensure
//! the OS will properly release any associated resources.
//!
//! ## MacOS/iOS (CoreBluetooth)
//!
//! Connections to devices are managed by the `Adapter` instance. You must call
//! [`connect_device`][Adapter::connect_device] before calling any methods on a `Device` (or child objects)
//! that require a connection or an error will be returned. Because the `Adapter` manages the connections, all
//! connections will be closed when the `Adapter` is dropped, therefore you must ensure the `Adapter` lives as long as
//! you need a connection to any of its devices.
//!
//! When you call [`disconnect_device`][Adapter::disconnect_device], access to that device will be terminated
//! for the application. If no other applications running on the system have connected to the device, the underlying
//! hardware connection will be closed.
//!
//! ## Windows (WinRT)
//!
//! Connections to devices are managed automatically by the OS. Calls to [`connect_device`][Adapter::connect_device]
//! and [`disconnect_device`][Adapter::disconnect_device] are no-ops that immediately return success. The actual
//! connection will be made as soon as a method is called on a `Device` that requires a connection (typically
//! [`discover_services`][Device::discover_services]). That connection will be maintained as long as the `Device`
//! instance or any child instance lives.
//!
//! # Feature flags
//!
//! The `serde` feature is available to enable serializing/deserializing device
//! identifiers.
//!
//! # Examples
//!
//! Examples demonstrating basic usage are available in the [examples folder].
//!
//! [examples folder]: https://github.com/alexmoon/bluest/tree/master/bluest/examples

pub mod btuuid;
pub mod error;
mod util;

#[cfg(target_os = "linux")]
mod bluer;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod corebluetooth;
#[cfg(target_os = "windows")]
mod windows;

use std::collections::HashMap;

#[cfg(target_os = "linux")]
pub use ::bluer::Uuid;
pub use btuuid::BluetoothUuidExt;
pub use error::Error;
pub use sys::adapter::Adapter;
pub use sys::characteristic::Characteristic;
pub use sys::descriptor::Descriptor;
pub use sys::device::{Device, DeviceId};
pub use sys::service::Service;
#[cfg(not(target_os = "linux"))]
pub use uuid::Uuid;

#[cfg(target_os = "linux")]
use crate::bluer as sys;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::corebluetooth as sys;
#[cfg(target_os = "windows")]
use crate::windows as sys;

/// Convenience alias for a result with [`Error`]
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Events generated by [`Adapter`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdapterEvent {
    /// The adapter has become available (powered on and ready to use)
    Available,
    /// The adapter has become unavailable (powered off or otherwise disabled)
    Unavailable,
}

/// Represents a device discovered during a scan operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisingDevice {
    /// The source of the advertisement
    pub device: crate::Device,
    /// The advertisment data
    pub adv_data: AdvertisementData,
    /// The signal strength in dBm of the received advertisement packet
    pub rssi: Option<i16>,
}

/// Data included in a Bluetooth advertisement or scan reponse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisementData {
    /// The (possibly shortened) local name of the device (CSS §A.1.2)
    pub local_name: Option<String>,
    /// Manufacturer specific data (CSS §A.1.4)
    pub manufacturer_data: Option<ManufacturerData>,
    /// Advertised GATT service UUIDs (CSS §A.1.1)
    pub services: Vec<Uuid>,
    /// Service associated data (CSS §A.1.11)
    pub service_data: HashMap<Uuid, Vec<u8>>,
    /// Transmitted power level (CSS §A.1.5)
    pub tx_power_level: Option<i16>,
    /// Set to true for connectable advertising packets
    pub is_connectable: bool,
}

/// Manufacturer specific data included in Bluetooth advertisements. See the Bluetooth Core Specification Supplement
/// §A.1.4 for details.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManufacturerData {
    /// Company identifier (defined [here](https://www.bluetooth.com/specifications/assigned-numbers/company-identifiers/))
    pub company_id: u16,
    /// Manufacturer specific data
    pub data: Vec<u8>,
}

/// GATT characteristic properties as defined in the Bluetooth Core Specification, Vol 3, Part G, §3.3.1.1.
/// Extended properties are also included as defined in §3.3.3.1.
#[allow(missing_docs)]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CharacteristicProperties {
    pub broadcast: bool,
    pub read: bool,
    pub write_without_response: bool,
    pub write: bool,
    pub notify: bool,
    pub indicate: bool,
    pub authenticated_signed_writes: bool,
    pub extended_properties: bool,
    pub reliable_write: bool,
    pub writable_auxiliaries: bool,
}

impl CharacteristicProperties {
    /// Raw transmutation from [`u32`].
    ///
    /// Extended properties are in the upper bits.
    pub fn from_bits(bits: u32) -> Self {
        CharacteristicProperties {
            broadcast: (bits & (1 << 0)) != 0,
            read: (bits & (1 << 1)) != 0,
            write_without_response: (bits & (1 << 2)) != 0,
            write: (bits & (1 << 3)) != 0,
            notify: (bits & (1 << 4)) != 0,
            indicate: (bits & (1 << 5)) != 0,
            authenticated_signed_writes: (bits & (1 << 6)) != 0,
            extended_properties: (bits & (1 << 7)) != 0,
            reliable_write: (bits & (1 << 8)) != 0,
            writable_auxiliaries: (bits & (1 << 9)) != 0,
        }
    }

    /// Raw transmutation to [`u32`].
    ///
    /// Extended properties are in the upper bits.
    pub fn to_bits(self) -> u32 {
        u32::from(self.broadcast)
            | (u32::from(self.read) << 1)
            | (u32::from(self.write_without_response) << 2)
            | (u32::from(self.write) << 3)
            | (u32::from(self.notify) << 4)
            | (u32::from(self.indicate) << 5)
            | (u32::from(self.authenticated_signed_writes) << 6)
            | (u32::from(self.extended_properties) << 7)
            | (u32::from(self.reliable_write) << 8)
            | (u32::from(self.writable_auxiliaries) << 9)
    }
}
