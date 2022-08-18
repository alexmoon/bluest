use std::collections::HashMap;

use futures::Stream;
use smallvec::SmallVec;
use tokio_stream::StreamExt;
use tracing::error;
use tracing::warn;
use uuid::Uuid;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementDataSection;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementReceivedEventArgs;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementWatcher;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementWatcherStoppedEventArgs;
use windows::Devices::Bluetooth::Advertisement::BluetoothLEManufacturerData;
use windows::Devices::Bluetooth::BluetoothAdapter;
use windows::Devices::Radios::Radio;
use windows::Devices::Radios::RadioState;
use windows::Foundation::Collections::IVector;
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::DataReader;

use crate::AdapterEvent;
use crate::AdvertisementData;
use crate::AdvertisingDevice;
use crate::BluetoothUuidExt;
use crate::Device;
use crate::ManufacturerData;
use crate::Result;

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be created with the [Adapter::default()] method.
pub struct Adapter {
    adapter: BluetoothAdapter,
}

impl TryFrom<BluetoothLEManufacturerData> for ManufacturerData {
    type Error = windows::core::Error;

    fn try_from(val: BluetoothLEManufacturerData) -> Result<Self, Self::Error> {
        let company_id = val.CompanyId()?;
        let buf = val.Data()?;
        let mut data = SmallVec::from_elem(0, buf.Length()? as usize);
        let reader = DataReader::FromBuffer(&buf)?;
        reader.ReadBytes(data.as_mut_slice())?;
        Ok(ManufacturerData { company_id, data })
    }
}

impl From<BluetoothLEAdvertisementReceivedEventArgs> for AdvertisementData {
    fn from(event_args: BluetoothLEAdvertisementReceivedEventArgs) -> Self {
        let is_connectable = event_args.IsConnectable().unwrap_or(false);
        let tx_power_level = event_args.TransmitPowerLevelInDBm().ok().and_then(|x| x.Value().ok());
        let (local_name, manufacturer_data, services, solicited_services, service_data) =
            if let Ok(adv) = event_args.Advertisement() {
                let local_name = adv
                    .LocalName()
                    .ok()
                    .and_then(|x| (!x.is_empty()).then(|| x.to_string_lossy()));
                let manufacturer_data = adv
                    .ManufacturerData()
                    .and_then(|x| x.GetAt(0))
                    .and_then(|x| x.try_into())
                    .ok();

                let services = adv
                    .ServiceUuids()
                    .map(|x| x.into_iter().map(|x| Uuid::from_u128(x.to_u128())).collect())
                    .unwrap_or_default();

                let (solicited_services, service_data) = if let Ok(data_sections) = adv.DataSections() {
                    (
                        to_solicited_services(&data_sections).unwrap_or_default(),
                        to_service_data(&data_sections).unwrap_or_default(),
                    )
                } else {
                    (Default::default(), Default::default())
                };

                (
                    local_name,
                    manufacturer_data,
                    services,
                    solicited_services,
                    service_data,
                )
            } else {
                (None, None, SmallVec::new(), SmallVec::new(), HashMap::new())
            };

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

#[derive(Debug, Clone, Copy)]
enum UuidKind {
    U16,
    U32,
    U128,
}

fn read_uuid(reader: &DataReader, kind: UuidKind) -> windows::core::Result<Uuid> {
    Ok(match kind {
        UuidKind::U16 => Uuid::from_u16(reader.ReadUInt16()?),
        UuidKind::U32 => Uuid::from_u32(reader.ReadUInt32()?),
        UuidKind::U128 => {
            let mut uuid = [0u8; 16];
            reader.ReadBytes(&mut uuid)?;
            Uuid::from_bytes(uuid)
        }
    })
}

fn to_solicited_services(
    data_sections: &IVector<BluetoothLEAdvertisementDataSection>,
) -> windows::core::Result<SmallVec<[Uuid; 1]>> {
    let mut solicited_services = SmallVec::new();

    for data in data_sections {
        let kind = match data.DataType()? {
            0x14 => Some(UuidKind::U16),
            0x15 => Some(UuidKind::U128),
            0x1f => Some(UuidKind::U32),
            _ => None,
        };

        if let Some(kind) = kind {
            let buf = data.Data()?;
            let reader = DataReader::FromBuffer(&buf)?;
            while let Ok(uuid) = read_uuid(&reader, kind) {
                solicited_services.push(uuid);
            }
        }
    }

    Ok(solicited_services)
}

fn to_service_data(
    data_sections: &IVector<BluetoothLEAdvertisementDataSection>,
) -> windows::core::Result<HashMap<Uuid, SmallVec<[u8; 16]>>> {
    let mut service_data = HashMap::new();

    for data in data_sections {
        let kind = match data.DataType()? {
            0x16 => Some(UuidKind::U16),
            0x20 => Some(UuidKind::U32),
            0x21 => Some(UuidKind::U128),
            _ => None,
        };

        if let Some(kind) = kind {
            let buf = data.Data()?;
            let reader = DataReader::FromBuffer(&buf)?;
            if let Ok(uuid) = read_uuid(&reader, kind) {
                let len = reader.UnconsumedBufferLength()? as usize;
                let mut value = SmallVec::from_elem(0, len);
                reader.ReadBytes(value.as_mut_slice())?;
                service_data.insert(uuid, value);
            }
        }
    }

    Ok(service_data)
}

impl Adapter {
    /// Creates the default adapter for the system
    pub async fn default() -> Option<Self> {
        let adapter = BluetoothAdapter::GetDefaultAsync().ok()?.await.ok()?;
        Some(Adapter { adapter })
    }

    /// A stream of [AdapterEvent] which allows the application to identify when the adapter is enabled or disabled.
    pub async fn events(&self) -> Result<impl Stream<Item = AdapterEvent> + '_> {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let radio = self.adapter.GetRadioAsync()?.await?;
        let token = radio.StateChanged(&TypedEventHandler::new(move |radio: &Option<Radio>, _| {
            let radio = radio.as_ref().expect("radio is null in StateChanged event");
            let state = radio.State().expect("radio state getter failed in StateChanged event");
            let _ = sender.blocking_send(if state == RadioState::On {
                AdapterEvent::Available
            } else {
                AdapterEvent::Unavailable
            });

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

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let radio = self.adapter.GetRadioAsync()?.await?;
        let events = self.events().await?;
        let state = radio.State()?;
        if state != RadioState::On {
            let _ = events.skip_while(|x| *x != AdapterEvent::Available).next().await;
        }
        Ok(())
    }

    /// Starts scanning for Bluetooth advertising packets.
    ///
    /// Returns a stream of [AdvertisingDevice] structs which contain the data from the advertising packet and the
    /// [Device] which sent it. Scanning is automatically stopped when the stream is dropped. Inclusion of duplicate
    /// packets is a platform-specific implementation detail.
    pub async fn scan<'a>(
        &'a self,
        services: Option<&'a [Uuid]>,
    ) -> Result<impl Stream<Item = AdvertisingDevice> + 'a> {
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

        Ok(received_events
            .map(|event_args| -> windows::core::Result<_> {
                // Parse relevant fields from event_args
                let addr = (event_args.BluetoothAddress()?, event_args.BluetoothAddressType()?);
                let rssi = event_args.RawSignalStrengthInDBm().ok();
                let adv_data = AdvertisementData::from(event_args);
                Ok((addr, rssi, adv_data))
            })
            .filter_map(move |res| {
                let _guard = &guard;

                // Filter by result and services
                match res {
                    Ok((addr, rssi, adv_data)) => {
                        if let Some(services) = &services {
                            adv_data
                                .services
                                .iter()
                                .any(|x| services.contains(x))
                                .then(|| (addr, rssi, adv_data))
                        } else {
                            Some((addr, rssi, adv_data))
                        }
                    }
                    Err(err) => {
                        warn!("Error getting bluetooth address from event: {:?}", err);
                        None
                    }
                }
            })
            .then(|(addr, rssi, adv_data)| {
                // Create the Device
                Box::pin(async move {
                    Device::from_addr(addr.0, addr.1)
                        .await
                        .map(|device| AdvertisingDevice { device, rssi, adv_data })
                })
            })
            .filter_map(move |res| match res {
                Ok(dev) => Some(dev),
                Err(err) => {
                    warn!("Error creating device: {:?}", err);
                    None
                }
            }))
    }

    /// Connects to the [Device]
    pub async fn connect(&self, device: &Device) -> Result<()> {
        device.connect().await
    }

    /// Disconnects from [Device]
    pub async fn disconnect(&self, device: &Device) -> Result<()> {
        device.disconnect().await
    }
}
