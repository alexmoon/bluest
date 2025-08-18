use crate::{AdvertisementData, ManufacturerData};

impl From<corebluetooth::advertisement_data::AdvertisementData> for AdvertisementData {
    fn from(value: corebluetooth::advertisement_data::AdvertisementData) -> Self {
        let services = value
            .service_uuids
            .into_iter()
            .chain(value.overflow_service_uuids)
            .map(Into::into)
            .collect();

        let service_data = value.service_data.into_iter().map(|(k, v)| (k.into(), v)).collect();

        AdvertisementData {
            local_name: value.local_name,
            manufacturer_data: value.manufacturer_data.map(Into::into),
            services,
            service_data,
            tx_power_level: value.tx_power_level,
            is_connectable: value.is_connectable,
        }
    }
}

impl From<corebluetooth::advertisement_data::ManufacturerData> for ManufacturerData {
    fn from(value: corebluetooth::advertisement_data::ManufacturerData) -> Self {
        ManufacturerData {
            company_id: value.company_id,
            data: value.data,
        }
    }
}
