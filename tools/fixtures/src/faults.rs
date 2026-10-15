//! P0 fault boundaries. Storage/root barriers are names reserved for P1 adapters;
//! only archive write/flush/response boundaries are exercised by today's child.
use std::io::{self, Write};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    BeforeWrite,
    AfterWrite,
    BeforeFlush,
    AfterFlush,
    BeforeResponse,
    AfterResponse,
    BeforeRename,
    AfterRename,
    BeforeDirectorySync,
    AfterDirectorySync,
    BeforeRootCommit,
    AfterRootCommit,
    BeforeLeaseChange,
    AfterLeaseChange,
}
impl Point {
    pub fn name(self) -> &'static str {
        match self {
            Self::BeforeWrite => "before-write",
            Self::AfterWrite => "after-write",
            Self::BeforeFlush => "before-flush",
            Self::AfterFlush => "after-flush",
            Self::BeforeResponse => "before-response",
            Self::AfterResponse => "after-response",
            Self::BeforeRename => "before-rename",
            Self::AfterRename => "after-rename",
            Self::BeforeDirectorySync => "before-directory-sync",
            Self::AfterDirectorySync => "after-directory-sync",
            Self::BeforeRootCommit => "before-root-commit",
            Self::AfterRootCommit => "after-root-commit",
            Self::BeforeLeaseChange => "before-lease-change",
            Self::AfterLeaseChange => "after-lease-change",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub point: Point,
    pub completed_bytes: u64,
}
pub trait Hook {
    fn hit(&mut self, boundary: Boundary) -> io::Result<()>;
}
pub struct ObservedWriter<W, H> {
    pub inner: W,
    pub hook: H,
    pub completed_bytes: u64,
}
impl<W: Write, H: Hook> Write for ObservedWriter<W, H> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.hook.hit(Boundary {
            point: Point::BeforeWrite,
            completed_bytes: self.completed_bytes,
        })?;
        let n = self.inner.write(b)?;
        self.completed_bytes = self
            .completed_bytes
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("byte counter overflow"))?;
        self.hook.hit(Boundary {
            point: Point::AfterWrite,
            completed_bytes: self.completed_bytes,
        })?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.hook.hit(Boundary {
            point: Point::BeforeFlush,
            completed_bytes: self.completed_bytes,
        })?;
        self.inner.flush()?;
        self.hook.hit(Boundary {
            point: Point::AfterFlush,
            completed_bytes: self.completed_bytes,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub enum WriteFault {
    Short(usize),
    Zero,
    Error(io::ErrorKind),
    InterruptOnce,
}
/// One reproducible fault at a specified write call; independent of the archive.
pub struct InjectedWriter<W> {
    pub inner: W,
    pub call: usize,
    pub at: usize,
    pub action: WriteFault,
}
impl<W: Write> Write for InjectedWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let call = self.call;
        self.call += 1;
        if call == self.at {
            match self.action {
                WriteFault::Short(n) => return self.inner.write(&b[..n.min(b.len())]),
                WriteFault::Zero => return Ok(0),
                WriteFault::Error(kind) => {
                    return Err(io::Error::new(kind, "injected write failure"));
                }
                WriteFault::InterruptOnce => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected interruption",
                    ));
                }
            }
        }
        self.inner.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
