use std::{
    fmt,
    io::Result,
    pin::Pin,
    slice,
    task::{Context, Poll},
    thread,
};

use java_spaghetti::{ByteArray, Global, Local, PrimitiveArray};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::runtime::Handle;
use tracing::{debug, warn};

use super::bindings::android::bluetooth::{BluetoothDevice, BluetoothSocket};
use super::bindings::java::io::{InputStream, OutputStream};
use super::OptionExt;

const BUFFER_CAPACITY: usize = 4096;
pub struct Channel {
    stream: Pin<Box<DuplexStream>>,
    channel: Global<BluetoothSocket>,
}

impl Channel {
    pub fn new(device: Global<BluetoothDevice>, psm: u16, secure: bool) -> crate::Result<Self> {
        let rt = tokio::runtime::Handle::current();

        device.vm().with_env(|env| {
            let device = device.as_local(env);

            let channel = if secure {
                device.createL2capChannel(psm as _)?.non_null()?
            } else {
                device.createInsecureL2capChannel(psm as _)?.non_null()?
            };

            channel.connect()?;

            let global_channel = channel.as_global();

            let (native_in_stream, native_out_stream) = tokio::io::duplex(BUFFER_CAPACITY);

            let (read_out, write_out) = tokio::io::split(native_out_stream);

            let input_stream = channel.getInputStream()?.non_null()?.as_global();
            let output_stream = channel.getOutputStream()?.non_null()?.as_global();

            // Unfortunately, Android's API for L2CAP channels is only blocking. Only way to deal with it
            // is to launch two background threads with blocking loops for reading and writing, which communicate
            // with the async Rust world via the stream channels.
            let read_rt = rt.clone();
            thread::spawn(move || Self::read_thread(input_stream, write_out, Box::pin(read_rt)));

            let transmit_size = usize::try_from(channel.getMaxTransmitPacketSize()?).unwrap();
            thread::spawn(move || Self::write_thread(output_stream, read_out, Box::pin(rt), transmit_size));

            Ok(Self {
                stream: Box::pin(native_in_stream),
                channel: global_channel,
            })
        })
    }

    //
    // The loops stop when either Android returns an error (for example if the channel is closed), or the
    // async channel gets closed because the user dropped the reader or writer structs.
    fn read_thread(
        input_stream: Global<InputStream>,
        mut write_output: impl AsyncWrite + Unpin,
        mut rt: Pin<Box<Handle>>,
    ) {
        debug!("l2cap read thread running!");

        input_stream.vm().with_env(|env| {
            let stream = input_stream.as_local(env);
            let arr: Local<ByteArray> = ByteArray::new(env, 1024);

            loop {
                match stream.read_byte_array(&arr) {
                    Ok(n) if n < 0 => {
                        warn!("failed to read from l2cap channel: {}", n);
                        break;
                    }
                    Err(e) => {
                        warn!("failed to read from l2cap channel: {:?}", e);
                        break;
                    }
                    Ok(n) => {
                        let n = n as usize;
                        let mut buf = vec![0u8; n];
                        arr.get_region(0, u8toi8_mut(&mut buf));

                        if let Err(e) = rt.as_mut().block_on(write_output.write_all(&mut buf)) {
                            warn!("failed to enqueue received l2cap packet: {:?}", e);
                            break;
                        }
                    }
                }
            }
        });

        debug!("l2cap read thread exiting!");
    }

    //
    // The loops stop when either Android returns an error (for example if the channel is closed), or the
    // async channel gets closed because the user dropped the reader or writer structs.

    fn write_thread(
        output_stream: Global<OutputStream>,
        mut read_output: impl AsyncRead + Unpin,
        rt: Pin<Box<Handle>>,
        transmit_size: usize,
    ) {
        debug!("l2cap write thread running!");

        output_stream.vm().with_env(|env| {
            let stream = output_stream.as_local(env);

            let mut buf = vec![0u8; transmit_size];

            loop {
                match rt.block_on(read_output.read(&mut buf)) {
                    Err(e) => {
                        warn!("failed to dequeue l2cap packet to send: {:?}", e);
                        break;
                    }
                    Ok(0) => {
                        debug!("End of stream reached");
                        break;
                    }
                    Ok(packet_size) => {
                        let b = ByteArray::new_from(env, u8toi8(&buf[..packet_size]));
                        if let Err(e) = stream.write_byte_array(b) {
                            warn!("failed to write to l2cap channel: {:?}", e);
                            break;
                        };
                    }
                }
            }
        });

        debug!("l2cap write thread exiting!");
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        self.channel.vm().with_env(|env| {
            let channel = self.channel.as_local(env);
            match channel.close() {
                Ok(()) => debug!("l2cap channel closed"),
                Err(e) => warn!("failed to close channel: {:?}", e),
            };
        });
    }
}

impl AsyncRead for Channel {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<()>> {
        self.stream.as_mut().poll_read(cx, buf)
    }
}

impl AsyncWrite for Channel {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        self.stream.as_mut().poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.stream.as_mut().poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        self.stream.as_mut().poll_shutdown(cx)
    }
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Channel")
            .field("stream", &self.stream)
            .field("channel", &"Android Bluetooth Channel")
            .finish()
    }
}

fn u8toi8(slice: &[u8]) -> &[i8] {
    let len = slice.len();
    let data = slice.as_ptr() as *const i8;
    // safety: any bit pattern is valid for u8 and i8, so transmuting them is fine.
    unsafe { slice::from_raw_parts(data, len) }
}

fn u8toi8_mut(slice: &mut [u8]) -> &mut [i8] {
    let len = slice.len();
    let data = slice.as_mut_ptr() as *mut i8;
    // safety: any bit pattern is valid for u8 and i8, so transmuting them is fine.
    unsafe { slice::from_raw_parts_mut(data, len) }
}
