use std::collections::HashMap;

use smallvec::SmallVec;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ManufacturerData {
    pub company_id: u16,
    pub data: SmallVec<[u8; 16]>,
}

#[derive(Debug, Clone)]
pub struct AdvertisementData {
    pub local_name: Option<String>,
    pub manufacturer_data: Option<ManufacturerData>,
    pub services: SmallVec<[Uuid; 1]>,
    pub solicited_services: SmallVec<[Uuid; 1]>,
    pub service_data: HashMap<Uuid, SmallVec<[u8; 16]>>,
    pub tx_power_level: Option<i16>,
    pub is_connectable: bool,
    // flags, peripheral connection interval range, appearance, public address, random address, advertising interval, uri, le supported features
}

pub struct DiscoveredDevice {
    pub device: crate::Device,
    pub adv_data: AdvertisementData,
    pub rssi: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Available,
    Unavailable,
}
