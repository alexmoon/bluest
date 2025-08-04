#![cfg(feature = "l2cap")]
use std::pin;
use std::task::{Context, Poll};

use futures_lite::io::{AsyncRead, AsyncWrite};

use crate::{sys, Result};

#[allow(unused)]
pub(crate) const PIPE_CAPACITY: usize = 0x100000; // 1Mb

#[cfg(not(target_os = "linux"))]
mod channel {
    use std::pin;
    use std::task::{Context, Poll};

    use futures_lite::io::{AsyncRead, AsyncWrite};

    use crate::{
        sys::l2cap_channel::{L2capChannelReader, L2capChannelWriter},
        Result,
    };

    /// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
    #[derive(Debug)]
    pub struct L2capChannel {
        pub(crate) reader: L2capChannelReader,
        pub(crate) writer: L2capChannelWriter,
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
            (reader, writer)
        }
    }

    impl AsyncRead for L2capChannel {
        fn poll_read(
            mut self: pin::Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
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
}

#[cfg(target_os = "linux")]
mod channel {
    use std::fmt::Debug;
    use std::pin;
    use std::task::{Context, Poll};

    use async_compat::Compat;
    use bluer::l2cap::Stream;
    use futures_lite::io::{AsyncRead, AsyncWrite};
    use tokio::io::AsyncWriteExt;

    use crate::{
        sys::l2cap_channel::{L2capChannelReader, L2capChannelWriter},
        Result,
    };

    /// A Bluetooth LE L2CAP Connection-oriented Channel (CoC)
    pub struct L2capChannel {
        pub(crate) stream: Compat<Stream>,
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
            self.stream.get_mut().shutdown().await?;
            Ok(())
        }

        /// Split the channel into read and write halves.
        #[inline]
        pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
            let (reader, writer) = self.stream.into_inner().into_split();
            (
                L2capChannelReader {
                    reader: Compat::new(reader),
                },
                L2capChannelWriter {
                    writer: Compat::new(writer),
                },
            )
        }

        /// Gets the Unerlying Stream type wich may support platform-specific additional functionality.
        ///
        /// Linux Only
        pub fn into_inner(self) -> Stream {
            self.stream.into_inner()
        }
    }

    impl AsyncRead for L2capChannel {
        fn poll_read(
            mut self: pin::Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            let stream = pin::pin!(&mut self.stream);
            stream.poll_read(cx, buf)
        }
    }

    impl AsyncWrite for L2capChannel {
        fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
            let stream = pin::pin!(&mut self.stream);
            stream.poll_write(cx, buf)
        }

        fn poll_flush(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            let stream = pin::pin!(&mut self.stream);
            stream.poll_flush(cx)
        }

        fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            let stream = pin::pin!(&mut self.stream);
            stream.poll_close(cx)
        }
    }

    impl Debug for L2capChannel {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(self.stream.get_ref(), f)
        }
    }
}

pub use channel::L2capChannel;

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
