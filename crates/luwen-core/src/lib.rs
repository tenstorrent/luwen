#[derive(Clone, Hash, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Grayskull,
    Wormhole,
    Unknown(u16),
}

impl Arch {
    pub fn is_wormhole(&self) -> bool {
        match self {
            Arch::Wormhole => true,
            _ => false,
        }
    }

    pub fn is_grayskull(&self) -> bool {
        match self {
            Arch::Grayskull => true,
            _ => false,
        }
    }
}
