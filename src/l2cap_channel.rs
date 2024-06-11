use std::{
    io::Result,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::sys;

/// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
#[derive(Debug)]
pub struct L2capChannel {
    pub(crate) channel: Pin<Box<sys::l2cap_channel::Channel>>,
}

impl AsyncRead for L2capChannel {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<()>> {
        self.channel.as_mut().poll_read(cx, buf)
    }
}

impl AsyncWrite for L2capChannel {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        self.channel.as_mut().poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.channel.as_mut().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.channel.as_mut().poll_shutdown(cx)
    }
}
