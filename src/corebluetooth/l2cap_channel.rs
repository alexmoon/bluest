use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use async_io::Async;
use corebluetooth::{L2capChannel, Peripheral};
use dispatch_executor::Handle;
use futures_lite::{AsyncReadExt, AsyncWriteExt};

use crate::Result;

/// The reader side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelReader {
    _channel: Handle<L2capChannel<Peripheral>>,
    stream: Arc<Async<UnixStream>>,
}

impl L2capChannelReader {
    /// Creates a new L2capChannelReader.
    pub(crate) fn new(channel: Handle<L2capChannel<Peripheral>>, stream: Arc<Async<UnixStream>>) -> Self {
        Self {
            _channel: channel,
            stream,
        }
    }

    /// Reads data from the L2CAP channel into the provided buffer.
    #[inline]
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        (&*self.stream).read(buf).await.map_err(|err| todo!())
    }

    /// Attempts to read data from the L2CAP channel into the provided buffer without blocking.
    #[inline]
    pub fn try_read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.stream.get_ref().read(buf).map_err(|err| todo!())
    }

    /// Closes the L2CAP channel reader.
    pub async fn close(&mut self) -> Result<()> {
        self.stream
            .get_ref()
            .shutdown(std::net::Shutdown::Read)
            .map_err(|err| todo!())
    }
}

/// The writer side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelWriter {
    _channel: Handle<L2capChannel<Peripheral>>,
    stream: Arc<Async<UnixStream>>,
}

impl L2capChannelWriter {
    /// Creates a new L2capChannelWriter.
    pub(crate) fn new(channel: Handle<L2capChannel<Peripheral>>, stream: Arc<Async<UnixStream>>) -> Self {
        Self {
            _channel: channel,
            stream,
        }
    }

    /// Writes data to the L2CAP channel.
    pub async fn write(&mut self, packet: &[u8]) -> Result<()> {
        (&*self.stream).write_all(packet).await.map_err(|err| todo!())
    }

    /// Attempts to write data to the L2CAP channel without blocking.
    pub fn try_write(&mut self, packet: &[u8]) -> Result<()> {
        self.stream.get_ref().write_all(packet).map_err(|err| todo!())
    }

    /// Closes the L2CAP channel writer.
    pub async fn close(&mut self) -> Result<()> {
        self.stream
            .get_ref()
            .shutdown(std::net::Shutdown::Write)
            .map_err(|err| todo!())
    }
}
