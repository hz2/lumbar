/// A byte address in the simulated address space.
pub type Address = u64;

/// Whether a memory access reads or writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessKind {
    Read,
    Write,
}

/// Identifies which logical array/stream an access belongs to (e.g. matmul's
/// A, B, and C), so simulation results can be broken down per stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamId(pub u16);

/// A single memory access: where, how big, what kind, and (optionally) which
/// logical stream it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub address: Address,
    pub size: u32,
    pub kind: AccessKind,
    pub stream: Option<StreamId>,
}

impl Access {
    pub fn read(address: Address, size: u32) -> Self {
        Self {
            address,
            size,
            kind: AccessKind::Read,
            stream: None,
        }
    }

    pub fn write(address: Address, size: u32) -> Self {
        Self {
            address,
            size,
            kind: AccessKind::Write,
            stream: None,
        }
    }

    #[must_use]
    pub fn with_stream(mut self, stream: StreamId) -> Self {
        self.stream = Some(stream);
        self
    }
}
