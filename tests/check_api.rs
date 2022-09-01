#![allow(clippy::let_unit_value)]

use bluest::*;
use futures_util::StreamExt;

fn assert_send<T: Send>(t: T) -> T {
    t
}

async fn check_adapter_apis(adapter: Adapter) -> Result<Device> {
    let events: Result<_> = assert_send(adapter.events()).await;
    let _event: Option<Result<AdapterEvent>> = assert_send(events?.next()).await;
    let _available: Result<()> = assert_send(adapter.wait_available()).await;

    let _devices: Result<Vec<Device>> = assert_send(adapter.connected_devices()).await;
    let devices: Result<Vec<Device>> =
        assert_send(adapter.connected_devices_with_services(&[btuuid::services::GENERIC_ACCESS])).await;

    let scan: Result<_> = assert_send(adapter.scan(&[btuuid::services::GENERIC_ACCESS])).await;
    let _adv: Option<AdvertisingDevice> = assert_send(scan?.next()).await;

    let discovery: Result<_> = assert_send(adapter.discover_devices(&[btuuid::services::GENERIC_ACCESS])).await;
    let _device: Option<Result<Device>> = assert_send(discovery?.next()).await;

    let device: Result<Device> = assert_send(adapter.open_device(&devices?[0].id())).await;

    let device = device?;
    let _res: Result<()> = assert_send(adapter.connect_device(&device)).await;
    let _res: Result<()> = assert_send(adapter.disconnect_device(&device)).await;

    Ok(device)
}

async fn check_device_apis(device: Device) -> Result<Service> {
    let _id: DeviceId = device.id();
    let _name: Option<String> = device.name();
    let _is_connected: bool = device.is_connected();

    #[cfg(target_os = "linux")]
    let _name: Option<String> = assert_send(device.name_async()).await;
    #[cfg(target_os = "linux")]
    let _is_connected: bool = assert_send(device.is_connected_async()).await;

    let _discovery: Result<Vec<Service>> = assert_send(device.discover_services()).await;
    let _discovery: Result<Vec<Service>> =
        assert_send(device.discover_services_with_uuid(btuuid::services::GENERIC_ACCESS)).await;
    let services: Result<Vec<Service>> = assert_send(device.services()).await;

    let _services_changed: Result<()> = assert_send(device.services_changed()).await;

    #[cfg(not(target_os = "windows"))]
    let _rssi: Result<i16> = assert_send(device.rssi()).await;

    Ok(services?.into_iter().next().unwrap())
}

async fn check_service_apis(service: Service) -> Result<Characteristic> {
    let _uuid: Uuid = service.uuid();

    #[cfg(target_os = "linux")]
    let _uuid: Result<Uuid> = assert_send(service.uuid_async()).await;

    #[cfg(not(target_os = "windows"))]
    let _is_primary: Result<bool> = assert_send(service.is_primary()).await;

    let _discovery: Result<Vec<Characteristic>> = assert_send(service.discover_characteristics()).await;
    let _discovery: Result<Vec<Characteristic>> =
        assert_send(service.discover_characteristics_with_uuid(btuuid::characteristics::DEVICE_NAME)).await;
    let characteristics: Result<Vec<Characteristic>> = assert_send(service.characteristics()).await;

    let _discovery: Result<Vec<Service>> = assert_send(service.discover_included_services()).await;
    let _discovery: Result<Vec<Service>> =
        assert_send(service.discover_included_services_with_uuid(btuuid::services::GENERIC_ACCESS)).await;
    let _services: Result<Vec<Service>> = assert_send(service.included_services()).await;

    Ok(characteristics?.into_iter().next().unwrap())
}

async fn check_characteristic_apis(characteristic: Characteristic) -> Result<Descriptor> {
    let _uuid: Uuid = characteristic.uuid();
    let _props: CharacteristicProperties = characteristic.properties();

    #[cfg(target_os = "linux")]
    let _uuid: Result<Uuid> = assert_send(characteristic.uuid_async()).await;
    #[cfg(target_os = "linux")]
    let _props: Result<CharacteristicProperties> = assert_send(characteristic.properties_async()).await;

    let _value: Result<Vec<u8>> = assert_send(characteristic.value()).await;
    let _value: Result<Vec<u8>> = assert_send(characteristic.read()).await;
    let _res: Result<()> = assert_send(characteristic.write(&[0u8])).await;
    let _res: () = assert_send(characteristic.write_without_response(&[0u8])).await;

    let notifications: Result<_> = assert_send(characteristic.notify()).await;
    let _notification: Option<Result<Vec<u8>>> = assert_send(notifications?.next()).await;
    let _is_notifying: Result<bool> = assert_send(characteristic.is_notifying()).await;

    let _discovery: Result<Vec<Descriptor>> = assert_send(characteristic.discover_descriptors()).await;
    let descriptors: Result<Vec<Descriptor>> = assert_send(characteristic.descriptors()).await;

    Ok(descriptors?.into_iter().next().unwrap())
}

async fn check_descriptor_apis(descriptor: Descriptor) -> Result<()> {
    let _uuid: Uuid = descriptor.uuid();

    #[cfg(target_os = "linux")]
    let _uuid: Result<Uuid> = assert_send(descriptor.uuid_async()).await;

    let _value: Result<Vec<u8>> = assert_send(descriptor.value()).await;
    let _value: Result<Vec<u8>> = assert_send(descriptor.read()).await;
    let _res: Result<()> = assert_send(descriptor.write(&[0u8])).await;

    Ok(())
}

#[allow(unused)]
async fn check_apis() -> Result<()> {
    let adapter: Option<Adapter> = assert_send(Adapter::default()).await;
    let device = check_adapter_apis(adapter.unwrap()).await?;
    let service = check_device_apis(device).await?;
    let characteristic = check_service_apis(service).await?;
    let descriptor = check_characteristic_apis(characteristic).await?;
    check_descriptor_apis(descriptor).await?;

    Ok(())
}

fn main() {}
