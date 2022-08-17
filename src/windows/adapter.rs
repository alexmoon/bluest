use std::collections::HashMap;

use futures::Stream;
use smallvec::SmallVec;
use tokio_stream::StreamExt;
use tracing::error;
use uuid::Uuid;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementReceivedEventArgs;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementWatcher;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementWatcherStoppedEventArgs;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEManufacturerData;
use windows::Devices::Bluetooth::BluetoothAdapter;
use windows::Devices::Radios::Radio;
use windows::Devices::Radios::RadioState;
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::DataReader;

use crate::AdvertisementData;
use crate::BluetoothUuidExt;
use crate::Device;
use crate::DiscoveredDevice;
use crate::Event;
use crate::ManufacturerData;
use crate::Result;

pub struct Adapter {
    adapter: BluetoothAdapter,
}

impl From<BluetoothLEManufacturerData> for ManufacturerData {
    fn from(val: BluetoothLEManufacturerData) -> Self {
        let company_id = val.CompanyId().unwrap();
        let buf = val.Data().unwrap();
        let mut data = SmallVec::from_elem(0, buf.Length().unwrap() as usize);
        let reader = DataReader::FromBuffer(&buf).unwrap();
        reader.ReadBytes(data.as_mut_slice()).unwrap();
        ManufacturerData { company_id, data }
    }
}

impl From<BluetoothLEAdvertisementReceivedEventArgs> for AdvertisementData {
    fn from(event_args: BluetoothLEAdvertisementReceivedEventArgs) -> Self {
        let is_connectable = event_args.IsConnectable().unwrap();
        let tx_power_level = event_args.TransmitPowerLevelInDBm().ok().map(|x| x.Value().unwrap());
        let adv = event_args.Advertisement().unwrap();
        let local_name = adv.LocalName().unwrap().to_string();
        let local_name = (!local_name.is_empty()).then(|| local_name);
        let manufacturer_data = adv.ManufacturerData().unwrap();
        let manufacturer_data =
            (manufacturer_data.Size().unwrap() > 0).then(|| manufacturer_data.GetAt(0).unwrap().into());

        let services = adv
            .ServiceUuids()
            .unwrap()
            .into_iter()
            .map(|x| Uuid::from_u128(x.to_u128()))
            .collect();

        let data_sections = adv.DataSections().unwrap();
        let mut solicited_services = SmallVec::new();
        let mut service_data = HashMap::new();
        for data in data_sections {
            match data.DataType().unwrap() {
                0x14 => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    while let Ok(n) = reader.ReadUInt16() {
                        solicited_services.push(Uuid::from_u16(n));
                    }
                }
                0x15 => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    let mut uuid = [0u8; 16];
                    while reader.ReadBytes(&mut uuid).is_ok() {
                        solicited_services.push(Uuid::from_bytes(uuid));
                    }
                }
                0x1f => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    while let Ok(n) = reader.ReadUInt32() {
                        solicited_services.push(Uuid::from_u32(n));
                    }
                }
                0x16 => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    if let Ok(uuid) = reader.ReadUInt16() {
                        let uuid = Uuid::from_u16(uuid);
                        let len = reader.UnconsumedBufferLength().unwrap() as usize;
                        let mut value = SmallVec::from_elem(0, len);
                        reader.ReadBytes(value.as_mut_slice()).unwrap();
                        service_data.insert(uuid, value);
                    }
                }
                0x20 => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    if let Ok(uuid) = reader.ReadUInt32() {
                        let uuid = Uuid::from_u32(uuid);
                        let len = reader.UnconsumedBufferLength().unwrap() as usize;
                        let mut value = SmallVec::from_elem(0, len);
                        reader.ReadBytes(value.as_mut_slice()).unwrap();
                        service_data.insert(uuid, value);
                    }
                }
                0x21 => {
                    let buf = data.Data().unwrap();
                    let reader = DataReader::FromBuffer(&buf).unwrap();
                    let mut uuid = [0u8; 16];
                    while reader.ReadBytes(&mut uuid).is_ok() {
                        let uuid = Uuid::from_bytes(uuid);
                        let len = reader.UnconsumedBufferLength().unwrap() as usize;
                        let mut value = SmallVec::from_elem(0, len);
                        reader.ReadBytes(value.as_mut_slice()).unwrap();
                        service_data.insert(uuid, value);
                    }
                }
                _ => (),
            }
        }

        AdvertisementData {
            local_name,
            manufacturer_data,
            services,
            tx_power_level,
            is_connectable,
            solicited_services,
            service_data,
        }
    }
}

impl From<BluetoothLEAdvertisementReceivedEventArgs> for DiscoveredDevice {
    fn from(event_args: BluetoothLEAdvertisementReceivedEventArgs) -> Self {
        let device = Device::new(event_args.BluetoothAddress().unwrap());
        let rssi = event_args.RawSignalStrengthInDBm().unwrap();
        let adv_data = event_args.into();

        DiscoveredDevice { device, rssi, adv_data }
    }
}

impl Adapter {
    pub(crate) async fn new() -> Result<Self> {
        let adapter = BluetoothAdapter::GetDefaultAsync().unwrap().await?;
        Ok(Adapter { adapter })
    }

    pub async fn events(&self) -> Result<impl Stream<Item = Event> + '_> {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let radio = self.adapter.GetRadioAsync().unwrap().await?;
        let token = radio.StateChanged(&TypedEventHandler::new(move |radio: &Option<Radio>, _| {
            let radio = radio.as_ref().unwrap();
            let state = radio.State().unwrap();
            if state == RadioState::On {
                sender.blocking_send(Event::Available).unwrap();
            } else {
                sender.blocking_send(Event::Unavailable).unwrap();
            }

            Ok(())
        }))?;

        let guard = scopeguard::guard((), move |_| {
            if let Err(err) = radio.RemoveStateChanged(token) {
                error!("Error removing state changed handler: {:?}", err);
            }
        });

        Ok(tokio_stream::wrappers::ReceiverStream::new(receiver).map(move |x| {
            let _guard = &guard;
            x
        }))
    }

    pub async fn wait_available(&self) -> Result<()> {
        let radio = self.adapter.GetRadioAsync()?.await?;
        let events = self.events().await?;
        let state = radio.State()?;
        if state != RadioState::On {
            let _ = events.skip_while(|x| *x != Event::Available).next().await;
        }
        Ok(())
    }

    pub async fn scan<'a>(&'a self, services: Option<&'a [Uuid]>) -> Result<impl Stream<Item = DiscoveredDevice> + 'a> {
        let watcher = BluetoothLEAdvertisementWatcher::new()?;
        watcher.SetAllowExtendedAdvertisements(true)?;

        let (sender, receiver) = tokio::sync::mpsc::channel(16);

        watcher.Received(&TypedEventHandler::new(
            move |_watcher, event_args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Some(event_args) = event_args {
                    let _ = sender.blocking_send(event_args.clone());
                }
                Ok(())
            },
        ))?;

        let received_events = tokio_stream::wrappers::ReceiverStream::new(receiver);
        let (received_events, abort_handle) = futures::stream::abortable(received_events);

        watcher.Stopped(&TypedEventHandler::new(
            move |_watcher, _event_args: &Option<BluetoothLEAdvertisementWatcherStoppedEventArgs>| {
                abort_handle.abort();
                Ok(())
            },
        ))?;

        watcher.Start()?;
        let guard = scopeguard::guard((), move |_| {
            if let Err(err) = watcher.Stop() {
                error!("Error stopping scan: {:?}", err);
            }
        });

        Ok(received_events.filter_map(move |x| {
            let _guard = &guard;
            let device = DiscoveredDevice::from(x);
            if let Some(services) = &services {
                device
                    .adv_data
                    .services
                    .iter()
                    .any(|x| services.contains(x))
                    .then(|| device)
            } else {
                Some(device)
            }
        }))
    }

    pub async fn connect(&self, device: &Device) -> Result<()> {
        Ok(())
    }

    pub async fn disconnect(&self, device: &Device) -> Result<()> {
        Ok(())
    }
}
