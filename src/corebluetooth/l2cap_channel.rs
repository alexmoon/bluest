use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::sync::Arc;

use async_io::Async;
use corebluetooth::{L2capChannel, Peripheral};
use dispatch_executor::Handle;
use futures_lite::{AsyncRead, AsyncWrite};

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

    /// Closes the L2CAP channel.
    pub async fn close(&mut self) -> std::io::Result<()> {
        self.stream.get_ref().shutdown(std::net::Shutdown::Both)
    }
}

impl Drop for L2capChannelReader {
    fn drop(&mut self) {
        let _ = self.stream.get_ref().shutdown(std::net::Shutdown::Read);
    }
}

impl AsyncRead for L2capChannelReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_read(cx, buf)
    }

    fn poll_read_vectored(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &mut [std::io::IoSliceMut<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_read_vectored(cx, bufs)
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

    /// Closes the L2CAP channel.
    pub async fn close(&mut self) -> std::io::Result<()> {
        self.stream.get_ref().shutdown(std::net::Shutdown::Both)
    }
}

impl Drop for L2capChannelWriter {
    fn drop(&mut self) {
        let _ = self.stream.get_ref().shutdown(std::net::Shutdown::Write);
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_close(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let stream = &mut &*self.stream;
        Pin::new(stream).poll_write_vectored(cx, bufs)
    }
}
