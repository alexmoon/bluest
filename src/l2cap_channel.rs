use std::pin;
use std::task::{Context, Poll};

use futures_lite::io::{AsyncRead, AsyncWrite};

use crate::sys;

#[allow(unused)]
pub(crate) const PIPE_CAPACITY: usize = 0x100000; // 1Mb

macro_rules! derive_async_read {
    ($type:ty, $field:tt) => {
        impl AsyncRead for $type {
            fn poll_read(
                mut self: pin::Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut [u8],
            ) -> Poll<std::io::Result<usize>> {
                let reader = pin::pin!(&mut self.$field);
                reader.poll_read(cx, buf)
            }
        }
    };
}

macro_rules! derive_async_write {
    ($type:ty, $field:tt) => {
        impl AsyncWrite for $type {
            fn poll_write(
                mut self: pin::Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<std::io::Result<usize>> {
                let writer = pin::pin!(&mut self.$field);
                writer.poll_write(cx, buf)
            }

            fn poll_flush(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
                let writer = pin::pin!(&mut self.$field);
                writer.poll_flush(cx)
            }

            fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
                let writer = pin::pin!(&mut self.$field);
                writer.poll_close(cx)
            }

            fn poll_write_vectored(
                mut self: pin::Pin<&mut Self>,
                cx: &mut Context<'_>,
                bufs: &[std::io::IoSlice<'_>],
            ) -> Poll<std::io::Result<usize>> {
                let writer = pin::pin!(&mut self.$field);
                writer.poll_write_vectored(cx, bufs)
            }
        }
    };
}

pub(crate) use {derive_async_read, derive_async_write};

/// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
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
