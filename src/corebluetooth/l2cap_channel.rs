use std::net::Shutdown;
use std::ops::Deref;
use std::os::unix::net::UnixStream;
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::{fmt, io};

use async_io::Async;
use corebluetooth::Peripheral;
use dispatch_executor::{DispatchCell, Ref};
use futures_lite::{AsyncRead, AsyncWrite};

use crate::l2cap_channel::{derive_async_read, derive_async_write};

#[derive(Clone)]
pub struct L2capChannel {
    pub(super) _channel: DispatchCell<corebluetooth::L2capChannel<Peripheral>>,
    pub(super) stream: Arc<Async<UnixStream>>,
    psm: u16,
}

impl fmt::Debug for L2capChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("psm: ")?;
        self.psm.fmt(f)
    }
}

impl L2capChannel {
    pub(crate) fn new(
        channel: Ref<'_, corebluetooth::L2capChannel<Peripheral>>,
        stream: Arc<Async<UnixStream>>,
    ) -> Self {
        let psm = channel.psm();
        Self {
            _channel: channel.retain(),
            stream,
            psm,
        }
    }

    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        (
            L2capChannelReader { channel: self.clone() },
            L2capChannelWriter { channel: self },
        )
    }
}

derive_async_read!(L2capChannel, stream.deref());

impl AsyncWrite for L2capChannel {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let writer = pin!(&mut self.stream.deref());
        writer.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let writer = pin!(&mut self.stream.deref());
        writer.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let writer = pin!(&mut self.stream.deref());
        ready!(writer.poll_close(cx))?;
        self.stream.get_ref().shutdown(Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let writer = pin!(&mut self.stream.deref());
        writer.poll_write_vectored(cx, bufs)
    }
}

/// The reader side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelReader {
    channel: L2capChannel,
}

impl Drop for L2capChannelReader {
    fn drop(&mut self) {
        let _ = self.channel.stream.get_ref().shutdown(Shutdown::Read);
    }
}

derive_async_read!(L2capChannelReader, channel);

/// The writer side of an L2CAP channel.
#[derive(Debug, Clone)]
pub struct L2capChannelWriter {
    channel: L2capChannel,
}

impl Drop for L2capChannelWriter {
    fn drop(&mut self) {
        let _ = self.channel.stream.get_ref().shutdown(Shutdown::Write);
    }
}

derive_async_write!(L2capChannelWriter, channel);
