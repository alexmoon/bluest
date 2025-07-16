#![cfg(feature = "l2cap")]
use std::pin;
use std::task::{Context, Poll};

use futures_lite::io::{AsyncRead, AsyncWrite};

use crate::{sys, Result};

pub(crate) const PIPE_CAPACITY: usize = 0x100000; // 1Mb

/// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
#[derive(Debug)]
pub struct L2capChannel {
    pub(crate) reader: sys::l2cap_channel::L2capChannelReader,
    pub(crate) writer: sys::l2cap_channel::L2capChannelWriter,
}

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

impl L2capChannel {
    /// Close the L2CAP channel.
    ///
    /// This closes the entire channel, in both directions (reading and writing).
    ///
    /// The channel is automatically closed when `L2capChannel` is dropped, so
    /// you don't need to call this explicitly.
    #[inline]
    pub async fn close(&mut self) -> Result<()> {
        self.writer.close().await
    }

    /// Split the channel into read and write halves.
    #[inline]
    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        let Self { reader, writer } = self;
        (L2capChannelReader { reader }, L2capChannelWriter { writer })
    }
}

impl AsyncRead for L2capChannel {
    fn poll_read(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        let reader = pin::pin!(&mut self.reader);
        reader.poll_read(cx, buf)
    }
}

impl AsyncWrite for L2capChannel {
    fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_write(cx, buf)
    }

    fn poll_flush(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_flush(cx)
    }

    fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_close(cx)
    }
}

impl L2capChannelReader {
    /// Close the L2CAP channel.
    ///
    /// This closes the entire channel, not just the read half.
    ///
    /// The channel is automatically closed when both the `L2capChannelWriter`
    /// and `L2capChannelReader` are dropped, so you don't need to call this explicitly.
    #[inline]
    pub async fn close(&mut self) -> Result<()> {
        self.reader.close().await
    }
}

impl AsyncRead for L2capChannelReader {
    fn poll_read(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        let reader = pin::pin!(&mut self.reader);
        reader.poll_read(cx, buf)
    }
}

impl L2capChannelWriter {
    /// Close the L2CAP channel.
    ///
    /// This closes the entire channel, not just the write half.
    ///
    /// The channel is automatically closed when both the `L2capChannelWriter`
    /// and `L2capChannelReader` are dropped, so you don't need to call this explicitly.
    #[inline]
    pub async fn close(&mut self) -> Result<()> {
        self.writer.close().await
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_write(cx, buf)
    }

    fn poll_flush(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_flush(cx)
    }

    fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_close(cx)
    }
}
