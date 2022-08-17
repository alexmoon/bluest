use windows::Devices::Bluetooth::{BluetoothAddressType, BluetoothLEDevice, GenericAttributeProfile::GattSession};

use crate::Result;

pub struct Device {
    device: BluetoothLEDevice,
    session: Option<GattSession>,
}

impl Device {
    pub(crate) async fn new(addr: u64, kind: BluetoothAddressType) -> Result<Self, windows::core::Error> {
        let device = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device { device, session: None })
    }

    pub(crate) async fn connect(&mut self) -> Result<()> {}
}
