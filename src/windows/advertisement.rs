use std::time::Duration;
use std::io; // Use std::io::Error as the error type
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::sync::Arc;

use futures_core::Stream;
use futures_lite::{stream, StreamExt};
use tracing::{debug, error, trace, warn};
use windows::core::HSTRING;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisement, BluetoothLEAdvertisementDataSection, BluetoothLEAdvertisementFilter,
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementType, BluetoothLEAdvertisementWatcher,
    BluetoothLEAdvertisementWatcherStoppedEventArgs, BluetoothLEManufacturerData, BluetoothLEScanningMode,
    BluetoothLEAdvertisementFlags, BluetoothLEAdvertisementPublisher,
};
use windows::Devices::Bluetooth::{BluetoothAdapter, BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationKind};
use windows::Devices::Radios::{Radio, RadioState};
use windows::Foundation::Collections::{IIterable, IVector};
use crate::error::{Error, ErrorKind};
use crate::{
    AdapterEvent, AdvertisementData, AdvertisingDevice, BluetoothUuidExt, ConnectionEvent, Device, DeviceId,
    ManufacturerData, Result, Uuid,
};
use windows::Storage::Streams::DataWriter;

pub struct AdvertisementImpl {
    company_id: u16,
}

impl AdvertisementImpl {
    pub fn new(company_id: u16) -> Self {
        AdvertisementImpl { company_id }
    }

    pub async fn advertise(&self, data: &Vec<u8>, advertise_duration: Option<Duration>) -> Result<(), io::Error> {
        let manufacturer_data = BluetoothLEManufacturerData::new()?;
        manufacturer_data.SetCompanyId(self.company_id)?;
        println!("Windows advertisement started with company ID: {:X}.", self.company_id);
        let writer = DataWriter::new()?;
        writer.WriteBytes(data)?;
    
        let buffer = writer.DetachBuffer()?;
        manufacturer_data.SetData(&buffer)?;
        let publisher = BluetoothLEAdvertisementPublisher::new()?;
        publisher.Advertisement()?.ManufacturerData()?.Append(&manufacturer_data)?;
        publisher.Start()?;
        let blue = BluetoothLEAdvertisement::new()?;
        blue.SetFlags(None)?;
        let manufacturer_data = BluetoothLEAdvertisementDataSection::new()?;
        let writer = DataWriter::new()?;
        writer.WriteBytes(&data)?;
        let buffer = writer.DetachBuffer()?;
        manufacturer_data.SetData(&buffer)?;
        blue.DataSections()?.Append(&manufacturer_data)?;

        let publisher = BluetoothLEAdvertisementPublisher::Create(&blue)?;
        publisher.Start()?;
        
        if let Some(duration) = advertise_duration {
            tokio::time::sleep(duration).await;
            self.stop()?;
            println!("Windows advertisement stopped after {:?}", duration);
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<(), io::Error> {
        println!("Windows advertisement manually stopped.");
        Ok(())
    }
}
