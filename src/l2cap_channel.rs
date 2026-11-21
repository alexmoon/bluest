use futures_lite::io::{AsyncRead, AsyncWrite};

use crate::sys;

#[allow(unused)]
pub(crate) const PIPE_CAPACITY: usize = 0x100000; // 1Mb

macro_rules! derive_async_read {
    ($type:ty, $($field:tt)*) => {
        #[allow(unused_mut)]
        impl AsyncRead for $type {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut [u8],
            ) -> std::task::Poll<std::io::Result<usize>> {
                let reader = std::pin::pin!(&mut self.$($field)*);
                reader.poll_read(cx, buf)
            }

            fn poll_read_vectored(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                bufs: &mut [std::io::IoSliceMut<'_>],
            ) -> std::task::Poll<std::io::Result<usize>> {
                let reader = std::pin::pin!(&mut self.$($field)*);
                reader.poll_read_vectored(cx, bufs)
            }
        }
    };
}

macro_rules! derive_async_write {
    ($type:ty, $($field:tt)+) => {
        #[allow(unused_mut)]
        impl AsyncWrite for $type {
            fn poll_write(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<std::io::Result<usize>> {
                let writer = std::pin::pin!(&mut self.$($field)+);
                writer.poll_write(cx, buf)
            }

            fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
                let writer = std::pin::pin!(&mut self.$($field)+);
                writer.poll_flush(cx)
            }

            fn poll_close(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
                let writer = std::pin::pin!(&mut self.$($field)+);
                writer.poll_close(cx)
            }

            fn poll_write_vectored(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                bufs: &[std::io::IoSlice<'_>],
            ) -> std::task::Poll<std::io::Result<usize>> {
                let writer = std::pin::pin!(&mut self.$($field)+);
                writer.poll_write_vectored(cx, bufs)
            }
        }
    };
}

#[allow(unused)]
pub(crate) use {derive_async_read, derive_async_write};

/// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
#[derive(Debug)]
pub struct L2capChannel(pub(super) sys::l2cap_channel::L2capChannel);

impl L2capChannel {
    /// Splits the channel into a read half and a write half
    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        let (reader, writer) = self.0.split();
        (L2capChannelReader { reader }, L2capChannelWriter { writer })
    }
}

derive_async_read!(L2capChannel, 0);
derive_async_write!(L2capChannel, 0);

/// Reader half of a L2CAP Connection-oriented Channel (CoC)
#[derive(Debug)]
pub struct L2capChannelReader {
    reader: sys::l2cap_channel::L2capChannelReader,
}

/// Writerhalf of a L2CAP Connection-oriented Channel (CoC)
#[derive(Debug)]
pub struct L2capChannelWriter {
    writer: sys::l2cap_channel::L2capChannelWriter,
}

derive_async_read!(L2capChannelReader, reader);

derive_async_write!(L2capChannelWriter, writer);
