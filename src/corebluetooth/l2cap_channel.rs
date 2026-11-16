use std::ops::Deref;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use async_io::Async;
use corebluetooth::Peripheral;
use dispatch_executor::Handle;
use futures_lite::{AsyncRead, AsyncWrite};

use crate::l2cap_channel::{derive_async_read, derive_async_write};

#[derive(Debug, Clone)]
pub struct L2capChannel {
    pub(super) channel: Handle<corebluetooth::L2capChannel<Peripheral>>,
    pub(super) stream: Arc<Async<UnixStream>>,
}

impl L2capChannel {
    pub(crate) fn new(
        channel: Handle<corebluetooth::L2capChannel<Peripheral>>,
        stream: Arc<Async<UnixStream>>,
    ) -> Self {
        Self { channel, stream }
    }

    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        (
            L2capChannelReader {
                _channel: self.channel.clone(),
                stream: self.stream.clone(),
            },
            L2capChannelWriter {
                _channel: self.channel,
                stream: self.stream,
            },
        )
    }
}

derive_async_read!(L2capChannel, stream.deref());
derive_async_write!(L2capChannel, stream.deref());

/// The reader side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelReader {
    _channel: Handle<corebluetooth::L2capChannel<Peripheral>>,
    stream: Arc<Async<UnixStream>>,
}

impl Drop for L2capChannelReader {
    fn drop(&mut self) {
        let _ = self.stream.get_ref().shutdown(std::net::Shutdown::Read);
    }
}

derive_async_read!(L2capChannelReader, stream.deref());

/// The writer side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelWriter {
    _channel: Handle<corebluetooth::L2capChannel<Peripheral>>,
    stream: Arc<Async<UnixStream>>,
}

impl Drop for L2capChannelWriter {
    fn drop(&mut self) {
        let _ = self.stream.get_ref().shutdown(std::net::Shutdown::Write);
    }
}

derive_async_write!(L2capChannelWriter, stream.deref());
