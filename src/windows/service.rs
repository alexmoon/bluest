use windows::Devices::Bluetooth::GenericAttributeProfile::GattDeviceService;

pub struct Service {
    service: GattDeviceService,
}

impl Service {
    pub(crate) fn new(service: GattDeviceService) -> Self {
        Service { service }
    }
}
