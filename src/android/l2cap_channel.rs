#![cfg(feature = "l2cap")]

use std::io::{Read, Write};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, pin, slice, thread};

use futures_lite::io::{AsyncRead, AsyncWrite, BlockOn};
use java_spaghetti::{ByteArray, Global, Local, PrimitiveArray};
use tracing::{debug, trace, warn};

use super::bindings::android::bluetooth::{BluetoothDevice, BluetoothSocket};
use super::vm_context::jni_with_env;
use super::OptionExt;
use crate::l2cap_channel::PIPE_CAPACITY;
use crate::Result;

pub fn open_l2cap_channel(
    device: Global<BluetoothDevice>,
    psm: u16,
    secure: bool,
) -> std::prelude::v1::Result<(L2capChannelReader, L2capChannelWriter), crate::Error> {
    jni_with_env(|env| {
        let device = device.as_local(env);

        let channel = if secure {
            device.createL2capChannel(psm as _)?.non_null()?
        } else {
            device.createInsecureL2capChannel(psm as _)?.non_null()?
        };

        channel.connect()?;

        // The L2capCloser closes the l2cap channel when dropped.
        // We put it in an Arc held by both the reader and writer, so it gets dropped
        // when
        let closer = Arc::new(L2capCloser {
            channel: channel.as_global(),
        });

        let (read_receiver, read_sender) = piper::pipe(PIPE_CAPACITY);
        let (write_receiver, write_sender) = piper::pipe(PIPE_CAPACITY);
        let input_stream = channel.getInputStream()?.non_null()?.as_global();
        let output_stream = channel.getOutputStream()?.non_null()?.as_global();

        // Unfortunately, Android's API for L2CAP channels is only blocking. Only way to deal with it
        // is to launch two background threads with blocking loops for reading and writing, which communicate
        // with the async Rust world via async channels.
        //
        // The loops stop when either Android returns an error (for example if the channel is closed), or the
        // async channel gets closed because the user dropped the reader or writer structs.
        thread::spawn(move || {
            debug!("l2cap read thread running!");
            let mut read_sender = BlockOn::new(read_sender);

            jni_with_env(|env| {
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
                            if let Err(e) = read_sender.write_all(&buf) {
                                warn!("failed to enqueue received l2cap packet: {:?}", e);
                                break;
                            }
                        }
                    }
                }
            });

            debug!("l2cap read thread exiting!");
        });

        thread::spawn(move || {
            debug!("l2cap write thread running!");
            let mut write_receiver = BlockOn::new(write_receiver);
            jni_with_env(|env| {
                let stream = output_stream.as_local(env);
                let mut buf = vec![0; PIPE_CAPACITY];

                loop {
                    match write_receiver.read(&mut buf) {
                        Err(e) => {
                            warn!("failed to dequeue l2cap packet to send: {:?}", e);
                            break;
                        }
                        Ok(0) => {
                            trace!("Stream ended");
                            break;
                        }
                        Ok(packet) => {
                            let b = ByteArray::new_from(env, u8toi8(&buf[..packet]));
                            if let Err(e) = stream.write_byte_array(b) {
                                warn!("failed to write to l2cap channel: {:?}", e);
                                break;
                            };
                        }
                    }
                }
            });

            debug!("l2cap write thread exiting!");
        });

        Ok((
            L2capChannelReader {
                closer: closer.clone(),
                stream: read_receiver,
            },
            L2capChannelWriter {
                closer,
                stream: write_sender,
            },
        ))
    })
}

/// Utility struct to close the channel on drop.
pub(super) struct L2capCloser {
    channel: Global<BluetoothSocket>,
}

impl L2capCloser {
    fn close(&self) {
        jni_with_env(|env| {
            let channel = self.channel.as_local(env);
            match channel.close() {
                Ok(()) => debug!("l2cap channel closed"),
                Err(e) => warn!("failed to close channel: {:?}", e),
            };
        });
    }
}

impl Drop for L2capCloser {
    fn drop(&mut self) {
        self.close()
    }
}

pub struct L2capChannelReader {
    stream: piper::Reader,
    closer: Arc<L2capCloser>,
}

impl L2capChannelReader {
    pub async fn close(&mut self) -> Result<()> {
        self.closer.close();
        Ok(())
    }
}

impl AsyncRead for L2capChannelReader {
    fn poll_read(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        let stream = pin::pin!(&mut self.stream);
        stream.poll_read(cx, buf)
    }
}

impl fmt::Debug for L2capChannelReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("L2capChannelReader")
    }
}

pub struct L2capChannelWriter {
    stream: piper::Writer,
    closer: Arc<L2capCloser>,
}

impl L2capChannelWriter {
    pub async fn close(&mut self) -> Result<()> {
        self.closer.close();
        Ok(())
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let stream = pin::pin!(&mut self.stream);
        let ret = stream.poll_write(cx, buf);
        ret
    }

    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        let stream = pin::pin!(&mut self.stream);
        stream.poll_flush(cx)
    }

    fn poll_close(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.closer.close();
        let stream = pin::pin!(&mut self.stream);
        stream.poll_close(cx)
    }
}

impl fmt::Debug for L2capChannelWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("L2capChannelWriter")
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
