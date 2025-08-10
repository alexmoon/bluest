#![cfg(feature = "l2cap")]

use std::pin::Pin;
use std::task::Context;

use futures_lite::io::{AsyncRead, AsyncWrite};

#[derive(Debug)]
pub struct L2capChannelReader {
    _private: (),
}

impl L2capChannelReader {
    pub async fn close(&mut self) -> crate::Result<()> {
        todo!()
    }
}

impl AsyncRead for L2capChannelReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        todo!()
    }
}
#[derive(Debug)]
pub struct L2capChannelWriter {
    _private: (),
}

impl L2capChannelWriter {
    pub async fn close(&mut self) -> crate::Result<()> {
        todo!()
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        todo!()
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        todo!()
    }
}
