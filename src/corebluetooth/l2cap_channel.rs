use core::ptr::NonNull;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::{fmt, pin};

use futures_lite::io::{AsyncRead, AsyncWrite, BlockOn};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass};
use objc2_core_bluetooth::CBL2CAPChannel;
use objc2_foundation::{
    NSDefaultRunLoopMode, NSInputStream, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol,
    NSOutputStream, NSRunLoop, NSStream, NSStreamDelegate, NSStreamEvent, NSString,
};
use tracing::{debug, trace, warn};

use super::dispatch::Dispatched;
use crate::l2cap_channel::PIPE_CAPACITY;
use crate::{derive_async_read, derive_async_write};

/// Utility struct to close the channel on drop.
pub(super) struct L2capCloser {
    channel: Dispatched<CBL2CAPChannel>,
}

impl L2capCloser {
    fn close(&self) {
        self.channel.dispatch(|channel| unsafe {
            if let Some(c) = channel.inputStream() {
                c.close()
            }
            if let Some(c) = channel.outputStream() {
                c.close()
            }
        })
    }
}

impl Drop for L2capCloser {
    fn drop(&mut self) {
        self.close()
    }
}

pub struct L2capChannel {
    pub(super) reader: L2capChannelReader,
    pub(super) writer: L2capChannelWriter,
}

impl L2capChannel {
    pub fn split(self) -> (L2capChannelReader, L2capChannelWriter) {
        (self.reader, self.writer)
    }
}

derive_async_read!(L2capChannel, reader);
derive_async_write!(L2capChannel, writer);

/// The reader side of an L2CAP channel.
pub struct L2capChannelReader {
    stream: piper::Reader,
    _closer: Arc<L2capCloser>,
    _delegate: Retained<InputStreamDelegate>,
}

impl L2capChannelReader {
    /// Creates a new L2capChannelReader.
    pub(crate) fn new(channel: Dispatched<CBL2CAPChannel>) -> Self {
        let (read_rx, read_tx) = piper::pipe(PIPE_CAPACITY);
        let closer = Arc::new(L2capCloser {
            channel: channel.clone(),
        });

        let delegate = channel.dispatch(|channel| unsafe {
            let input_stream = channel.inputStream().unwrap();
            let delegate = InputStreamDelegate::new(read_tx);
            input_stream.setDelegate(Some(&ProtocolObject::from_retained(delegate.clone())));
            input_stream.scheduleInRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSDefaultRunLoopMode);
            input_stream.open();
            delegate
        });

        Self {
            stream: read_rx,
            _delegate: delegate,
            _closer: closer,
        }
    }
}

derive_async_read!(L2capChannelReader, stream);

impl fmt::Debug for L2capChannelReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("L2capChannelReader")
    }
}

/// The writer side of an L2CAP channel.
pub struct L2capChannelWriter {
    stream: piper::Writer,
    closer: Arc<L2capCloser>,
    _delegate: Retained<OutputStreamDelegate>,
}

impl L2capChannelWriter {
    /// Creates a new L2capChannelWriter.
    pub(crate) fn new(channel: Dispatched<CBL2CAPChannel>) -> Self {
        let (write_rx, write_tx) = piper::pipe(PIPE_CAPACITY);
        let closer = Arc::new(L2capCloser {
            channel: channel.clone(),
        });

        let delegate = channel.dispatch(|channel| unsafe {
            let output_stream = channel.outputStream().unwrap();
            let delegate = OutputStreamDelegate::new(write_rx, Dispatched::retain(&output_stream));
            output_stream.setDelegate(Some(&ProtocolObject::from_retained(delegate.clone())));
            output_stream.scheduleInRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSDefaultRunLoopMode);
            output_stream.open();

            let center = NSNotificationCenter::defaultCenter();
            let name = NSString::from_str("ChannelWriteNotification");
            center.addObserver_selector_name_object(&delegate, sel!(onNotified:), Some(&name), None);
            delegate
        });

        Self {
            stream: write_tx,
            _delegate: delegate,
            closer,
        }
    }

    fn notify(&self) {
        unsafe {
            let name = NSString::from_str("ChannelWriteNotification");
            let center = NSNotificationCenter::defaultCenter();
            center.postNotificationName_object(&name, None);
        }
    }
}

impl AsyncWrite for L2capChannelWriter {
    fn poll_write(mut self: pin::Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let stream = pin::pin!(&mut self.stream);
        let ret = stream.poll_write(cx, buf);
        if matches!(ret, Poll::Ready(Ok(_))) {
            self.notify();
        }
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

struct InputStreamDelegateIvars {
    writer: Mutex<BlockOn<piper::Writer>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = InputStreamDelegateIvars]
    #[derive(PartialEq, Eq, Hash)]
    struct InputStreamDelegate;

    unsafe impl NSObjectProtocol for InputStreamDelegate {}

    unsafe impl NSStreamDelegate for InputStreamDelegate {
        #[unsafe(method(stream:handleEvent:))]
        fn handle_event(&self, stream: &NSStream, event_code: NSStreamEvent) {
            let input_stream = stream.downcast_ref::<NSInputStream>().unwrap();
            if let NSStreamEvent::HasBytesAvailable = event_code {
                // This is the only writer task, so there should never be contention on this lock
                let mut writer = self.ivars().writer.try_lock().unwrap();
                // This is the the only task that writes to the pipe so at least this many bytes will be available
                let to_fill = writer.get_ref().capacity() - writer.get_ref().len();
                let mut buf = vec![0u8; to_fill].into_boxed_slice();
                let res = unsafe { input_stream.read_maxLength(NonNull::new_unchecked(buf.as_mut_ptr()), buf.len()) };
                if res < 0 {
                    debug!("Read Loop Error: Stream read failed");
                    return;
                }
                let filled = res.try_into().unwrap();
                if let Err(e) = writer.write_all(&buf[..filled]) {
                    debug!("Read Loop Error: {:?}", e);
                    unsafe {
                        input_stream.setDelegate(None);
                        input_stream.close();
                    }
                }
            }
        }
    }
);

impl InputStreamDelegate {
    pub fn new(writer: piper::Writer) -> Retained<Self> {
        let ivars = InputStreamDelegateIvars {
            writer: Mutex::new(BlockOn::new(writer)),
        };
        let this = InputStreamDelegate::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }
}

struct OutputStreamDelegateIvars {
    receiver: Mutex<BlockOn<piper::Reader>>,
    stream: Dispatched<NSOutputStream>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = OutputStreamDelegateIvars]
    #[derive(PartialEq, Eq, Hash)]
    struct OutputStreamDelegate;

    unsafe impl NSObjectProtocol for OutputStreamDelegate {}

    unsafe impl NSStreamDelegate for OutputStreamDelegate {
        #[unsafe(method(stream:handleEvent:))]
        fn handle_event(&self, stream: &NSStream, event_code: NSStreamEvent) {
            let output_stream = stream.downcast_ref::<NSOutputStream>().unwrap();
            if let NSStreamEvent::HasSpaceAvailable = event_code {
                self.send_packet(output_stream)
            }
        }

        #[unsafe(method(onNotified:))]
        fn on_notified(&self, _n: &NSNotification) {
            let stream = unsafe { self.ivars().stream.get() };
            self.send_packet(stream)
        }
    }
);

impl OutputStreamDelegate {
    pub fn new(receiver: piper::Reader, stream: Dispatched<NSOutputStream>) -> Retained<Self> {
        let ivars = OutputStreamDelegateIvars {
            receiver: Mutex::new(BlockOn::new(receiver)),
            stream,
        };
        let this = OutputStreamDelegate::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    fn send_packet(&self, output_stream: &NSOutputStream) {
        let mut receiver = self.ivars().receiver.lock().unwrap();

        // This is racy but there will always be at least this many bytesin the channel
        let to_write = receiver.get_ref().len();
        if to_write == 0 {
            trace!("No data to write");
            return;
        }
        let mut buf = vec![0u8; to_write];
        let to_write = match receiver.read(&mut buf) {
            Err(e) => {
                warn!("Error reading from stream {:?}", e);
                return;
            }
            Ok(0) => {
                trace!("No more data to write");
                self.close(output_stream);
                return;
            }
            Ok(n) => n,
        };

        buf.truncate(to_write);
        let res = unsafe { output_stream.write_maxLength(NonNull::new_unchecked(buf.as_mut_ptr()), buf.len()) };
        if res < 0 {
            debug!("Write Loop Error: Stream write failed");
            self.close(output_stream);
        }
    }

    fn close(&self, output_stream: &NSOutputStream) {
        unsafe {
            output_stream.setDelegate(None);
            output_stream.close();
            let center = NSNotificationCenter::defaultCenter();
            center.removeObserver(self);
        }
    }
}
