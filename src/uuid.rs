use uuid::Uuid;

pub const BLUETOOTH_BASE_UUID: u128 = 0x00000000_0000_1000_8000_00805f9b34fb;

pub const fn bluetooth_uuid_from_u16(uuid: u16) -> Uuid {
    Uuid::from_u128(((uuid as u128) << 96) | BLUETOOTH_BASE_UUID)
}

pub const fn bluetooth_uuid_from_u32(uuid: u32) -> Uuid {
    Uuid::from_u128(((uuid as u128) << 96) | BLUETOOTH_BASE_UUID)
}

pub trait BluetoothUuidExt {
    fn from_u16(uuid: u16) -> Self;
    fn from_u32(uuid: u32) -> Self;
    fn from_bluetooth_bytes(bytes: &[u8]) -> Self;

    fn is_u16_uuid(&self) -> bool;
    fn is_u32_uuid(&self) -> bool;

    fn try_to_u16(&self) -> Option<u16>;
    fn try_to_u32(&self) -> Option<u32>;

    fn as_bluetooth_bytes(&self) -> &[u8];
}

impl BluetoothUuidExt for Uuid {
    fn from_u16(uuid: u16) -> Self {
        bluetooth_uuid_from_u16(uuid)
    }

    fn from_u32(uuid: u32) -> Self {
        bluetooth_uuid_from_u32(uuid)
    }

    fn from_bluetooth_bytes(bytes: &[u8]) -> Self {
        match bytes.len() {
            2 => Self::from_u16(u16::from_be_bytes(bytes.try_into().unwrap())),
            4 => Self::from_u32(u32::from_be_bytes(bytes.try_into().unwrap())),
            16 => Self::from_bytes(bytes.try_into().unwrap()),
            _ => panic!("Invalid byte slice length {}", bytes.len()),
        }
    }

    fn is_u16_uuid(&self) -> bool {
        let u = self.as_u128();
        (u & ((1 << 96) - 1)) == BLUETOOTH_BASE_UUID && (((u >> 96) as u32) & 0xffff0000) == 0
    }

    fn is_u32_uuid(&self) -> bool {
        let u = self.as_u128();
        (u & ((1 << 96) - 1)) == BLUETOOTH_BASE_UUID && (((u >> 96) as u32) & 0xffff0000) != 0
    }

    fn try_to_u16(&self) -> Option<u16> {
        let u = self.as_u128();
        self.is_u16_uuid().then(|| (u >> 96) as u16)
    }

    fn try_to_u32(&self) -> Option<u32> {
        let u = self.as_u128();
        self.is_u32_uuid().then(|| (u >> 96) as u32)
    }

    fn as_bluetooth_bytes(&self) -> &[u8] {
        let bytes = &*self.as_bytes();
        if self.is_u16_uuid() {
            &bytes[2..4]
        } else if self.is_u32_uuid() {
            &bytes[0..4]
        } else {
            &bytes[..]
        }
    }
}
