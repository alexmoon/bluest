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
    publisher: Option<BluetoothLEAdvertisementPublisher>,
    company_id: u16,
}

impl AdvertisementImpl {
    /// Creates a new `Advertisement` instance with the specified company ID.
    pub fn new(company_id: u16) -> Self {
        Self {
            publisher: None, // Initialize without publisher
            company_id,
        }
    }

    pub async fn advertise(&mut self, data: &Vec<u8>, advertise_duration: Option<Duration>) -> Result<(), io::Error> {

        // Start the publisher if it exists
        if let Some(publisher) = &self.publisher {
            publisher.Stop()?;
            self.publisher=None;
        }

        if self.publisher.is_none() {
            // Initialize BluetoothLEAdvertisement and publisher if not already created
            let manufacturer_data = BluetoothLEManufacturerData::new()?;
            manufacturer_data.SetCompanyId(self.company_id)?;
            println!("Windows advertisement started with company ID: {:X}.", self.company_id);
            let writer = DataWriter::new()?;
            writer.WriteBytes(data)?;
        
            let buffer = writer.DetachBuffer()?;
            manufacturer_data.SetData(&buffer)?;
            
            let blue = BluetoothLEAdvertisement::new()?;
            // blue.SetFlags(None)?;
            //let manufacturer_data_section = BluetoothLEAdvertisementDataSection::new()?;
          //  manufacturer_data_section.SetData(&buffer)?;
            //blue.DataSections()?.Append(&manufacturer_data_section)?;

            // Create the publisher and start advertising
            //let publisher = BluetoothLEAdvertisementPublisher::Create(&blue)?;
            let publisher = BluetoothLEAdvertisementPublisher::new()?;
            publisher.Advertisement()?.ManufacturerData()?.Append(&manufacturer_data)?;
            //  publisher.Start()?; // Start the publisher before assigning it to `self.publisher`
    
            // Assign the successfully started publisher to `self.publisher`
            self.publisher = Some(publisher);
        } 
        

        if let Some(publisher) = &self.publisher {
            println!("{:?}",publisher.Status());
            publisher.Start()?;
        }

        if let Some(duration) = advertise_duration {
            tokio::time::sleep(duration).await;
            if let Some(publisher) = &self.publisher {
                publisher.Stop()?; // Stop the advertisement
                self.publisher = None; // Clear the publisher to ensure it can be restarted if needed
            }
            println!("Windows advertisement stopped after {:?}", duration);
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), io::Error> {
        println!("Windows advertisement manually stopped.");
        if let Some(publisher) = &self.publisher {
            publisher.Stop()?; // Stop the advertisement
            self.publisher = None; // Clear the publisher to ensure it can be restarted if needed
        }
        Ok(())
    }
}
