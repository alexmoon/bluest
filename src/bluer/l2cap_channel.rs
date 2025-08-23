#![cfg(feature = "l2cap")]

use std::fmt::Debug;
use std::pin;
use std::task::{Context, Poll};

use async_compat::Compat;
use bluer::l2cap::stream::{OwnedReadHalf, OwnedWriteHalf};
use bluer::l2cap::Stream;
use futures_lite::io::{AsyncRead, AsyncWrite};

pub struct L2capChannel(pub(super) Compat<Stream>);

impl L2capChannel {
    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        let (reader, writer) = self.0.into_inner().into_split();
        let (reader, writer) = (Compat::new(reader), Compat::new(writer));
        (L2capChannelReader { reader }, L2capChannelWriter { writer })
    }
}

derive_async_read!(L2capChannel, 0);
derive_async_write!(L2capChannel, 0);

pub struct L2capChannelReader {
    pub(crate) reader: Compat<OwnedReadHalf>,
}

derive_async_read!(L2capChannelReader, reader);

impl Debug for L2capChannelReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.reader.get_ref(), f)
    }
}

pub struct L2capChannelWriter {
    pub(crate) writer: Compat<OwnedWriteHalf>,
}

derive_async_write!(L2capChannelWriter, writer);

impl Debug for L2capChannelWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.writer.get_ref(), f)
    }
}
