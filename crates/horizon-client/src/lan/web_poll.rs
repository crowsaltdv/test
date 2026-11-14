//! What the network threads poll in a browser (Emscripten, docs/web.md), in place of mio, which
//! has no selector there. A page can't accept a TCP connection, so the only connection a thread
//! serves is the integrated server's own player, over its in-memory pipe, and the pipe wakes
//! the thread itself ([`Waker`]): a poll waits for that wake or its timeout, and reports no
//! socket. The names are mio's, as `io_pool.rs` uses them.
use std::{
    io,
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token(pub usize);

/// What a socket is watched for. Nothing is watched here.
#[derive(Clone, Copy)]
pub struct Interest;
impl Interest {
    pub const READABLE: Self = Self;
    pub const WRITABLE: Self = Self;
}
impl std::ops::BitOr for Interest {
    type Output = Self;
    fn bitor(self, _: Self) -> Self {
        self
    }
}

/// Whether a wake is waiting for the poll, and what the poll sleeps on.
type Wake = Arc<(Mutex<bool>, Condvar)>;

pub struct Registry(Wake);
impl Registry {
    /// No socket can be polled here: the connection is refused with this error.
    pub fn register(&self, _: &mut net::TcpStream, _: Token, _: Interest) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub fn deregister(&self, _: &mut net::TcpStream) -> io::Result<()> {
        Ok(())
    }
}

pub struct Poll(Registry);
impl Poll {
    pub fn new() -> io::Result<Self> {
        Ok(Self(Registry(Wake::default())))
    }
    pub fn registry(&self) -> &Registry {
        &self.0
    }
    /// Waits for a wake, at most `timeout`. No socket is ready, ever: `events` stays empty.
    pub fn poll(&mut self, _events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        let (woken, wake) = &*self.0.0;
        let mut woken = woken.lock().unwrap_or_else(PoisonError::into_inner);
        if !*woken {
            woken = match timeout {
                Some(timeout) => {
                    wake.wait_timeout(woken, timeout)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
                None => wake.wait(woken).unwrap_or_else(PoisonError::into_inner),
            };
        }
        *woken = false;
        Ok(())
    }
}

pub struct Waker(Wake);
impl Waker {
    pub fn new(registry: &Registry, _: Token) -> io::Result<Self> {
        Ok(Self(registry.0.clone()))
    }
    pub fn wake(&self) -> io::Result<()> {
        let (woken, wake) = &*self.0;
        *woken.lock().unwrap_or_else(PoisonError::into_inner) = true;
        wake.notify_one();
        Ok(())
    }
}

/// A ready socket: there is never one.
pub struct Event(Token);
impl Event {
    pub fn token(&self) -> Token {
        self.0
    }
    pub fn is_readable(&self) -> bool {
        false
    }
    pub fn is_writable(&self) -> bool {
        false
    }
    pub fn is_read_closed(&self) -> bool {
        false
    }
    pub fn is_write_closed(&self) -> bool {
        false
    }
    pub fn is_error(&self) -> bool {
        false
    }
}

pub struct Events;
impl Events {
    pub fn with_capacity(_: usize) -> Self {
        Self
    }
}
impl<'a> IntoIterator for &'a Events {
    type Item = &'a Event;
    type IntoIter = std::iter::Empty<&'a Event>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::empty()
    }
}

pub mod net {
    use std::io::{self, Read, Write};

    /// A TCP stream that [`super::Registry::register`] refused: never read or written.
    pub struct TcpStream(std::net::TcpStream);
    impl TcpStream {
        pub fn from_std(stream: std::net::TcpStream) -> Self {
            Self(stream)
        }
        pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
            self.0.shutdown(how)
        }
    }
    impl Read for TcpStream {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.0.read(out)
        }
    }
    impl Write for TcpStream {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.0.write(data)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
}
