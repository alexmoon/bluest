use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

impl L2capCloser {
    fn close(&self) {
        self.channel.dispatch(|channel| unsafe {
            if let Some(c) = channel.inputStream() {
                c.close()
            }
            if let Some(c) = channel.outputStream() {
                c.close()
            }
        })
    }
}
use async_io::Async;
use corebluetooth::{L2capChannel, Peripheral};
use dispatch_executor::Handle;
use futures_lite::{AsyncReadExt, AsyncWriteExt};

impl Drop for L2capCloser {
    fn drop(&mut self) {
        self.close()
    }
}
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

    fn send_packet(&self, output_stream: &NSOutputStream) {
        let mut receiver = self.ivars().receiver.lock().unwrap();

        // This is racy but there will always be at least this many bytesin the channel
        let to_write = receiver.get_ref().len();
        if to_write == 0 {
            trace!("No data to write");
            return;
        }
        let mut buf = vec![0u8; to_write];
        let to_write = match receiver.read(&mut buf) {
            Err(e) => {
                warn!("Error reading from stream {:?}", e);
                return;
            }
            Ok(0) => {
                trace!("No more data to write");
                self.close(output_stream);
                return;
            }
            Ok(n) => n,
        };

        buf.truncate(to_write);
        let res = unsafe { output_stream.write_maxLength(NonNull::new_unchecked(buf.as_mut_ptr()), buf.len()) };
        if res < 0 {
            debug!("Write Loop Error: Stream write failed");
            self.close(output_stream);
        }
    }

    fn close(&self, output_stream: &NSOutputStream) {
        unsafe {
            output_stream.setDelegate(None);
            output_stream.close();
            let center = NSNotificationCenter::defaultCenter();
            center.removeObserver(self);
        }
    }
}
