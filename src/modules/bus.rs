#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventKind {
    Frame,
    Update,
    Key,
    Open,
    Close,
    Config,
}

impl EventKind {
    pub const fn flag(self) -> u32 {
        match self {
            EventKind::Frame => 1 << 0,
            EventKind::Update => 1 << 1,
            EventKind::Key => 1 << 2,
            EventKind::Open => 1 << 3,
            EventKind::Close => 1 << 4,
            EventKind::Config => 1 << 5,
        }
    }
}
