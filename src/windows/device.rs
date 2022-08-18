use windows::Devices::Bluetooth::{BluetoothAddressType, BluetoothLEDevice, GenericAttributeProfile::GattSession};

use crate::Result;

pub struct Device {
    addr: (u64, BluetoothAddressType),
    device: Option<BluetoothLEDevice>,
    session: Option<GattSession>,
}

impl Device {
    pub(crate) async fn new(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let device = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device {
            addr: (addr, kind),
            device: Some(device),
            session: None,
        })
    }

    pub(crate) async fn connect(&mut self) -> Result<(), windows::core::Error> {
        // if self.session.is_none() {
        //     let session = GattSession::FromDeviceIdAsync(self.device.DeviceId()?);
        // }
        Ok(())
    }
}
