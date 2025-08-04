#![cfg(feature = "l2cap")]

use std::fmt::Debug;
use std::pin;
use std::task::{Context, Poll};

use async_compat::Compat;
use bluer::l2cap::stream::{OwnedReadHalf, OwnedWriteHalf};
use futures_lite::io::{AsyncRead, AsyncWrite};

pub struct L2capChannelReader {
    pub(crate) reader: Compat<OwnedReadHalf>,
}

impl L2capChannelReader {
    pub async fn close(&mut self) -> crate::Result<()> {
        todo!()
    }
}

impl AsyncRead for L2capChannelReader {
    fn poll_read(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        let reader = pin::pin!(&mut self.reader);
        reader.poll_read(cx, buf)
    }
}

impl Debug for L2capChannelReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.reader.get_ref(), f)
    }
}

pub struct L2capChannelWriter {
    pub(crate) writer: Compat<OwnedWriteHalf>,
}

impl L2capChannelWriter {
    pub async fn close(&mut self) -> crate::Result<()> {
        todo!()
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_write(cx, buf)
    }

    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_flush(cx)
    }

    fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let writer = pin::pin!(&mut self.writer);
        writer.poll_close(cx)
    }
}

impl Debug for L2capChannelWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.writer.get_ref(), f)
    }
}
