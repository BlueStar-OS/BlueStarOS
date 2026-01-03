use core::{error::Error, fmt::{Display, Formatter, Result}};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsFsError {
    Mounted,
    Unmounted,
    IO,
    BrokenPipe,
    MountFail,
    UnmountFail,
    NotFound,
    AlreadyExists,
    NotDir,
    IsDir,
    Invalid,
    BadFd,
    PermissionDenied,
    NotSupported,
    Busy,
    NoSpace,
}



impl Display for VfsFsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Mounted => write!(f, "Mounted"),
            Self::Unmounted => write!(f, "Unmounted"),
            Self::IO => write!(f, "IO"),
            Self::BrokenPipe => write!(f, "BrokenPipe"),
            Self::MountFail => write!(f, "MountFail"),
            Self::UnmountFail => write!(f, "UnmountFail"),
            Self::NotFound => write!(f, "NotFound"),
            Self::AlreadyExists => write!(f, "AlreadyExists"),
            Self::NotDir => write!(f, "NotDir"),
            Self::IsDir => write!(f, "IsDir"),
            Self::Invalid => write!(f, "Invalid"),
            Self::BadFd => write!(f, "BadFd"),
            Self::PermissionDenied => write!(f, "PermissionDenied"),
            Self::NotSupported => write!(f, "NotSupported"),
            Self::Busy => write!(f, "Busy"),
            Self::NoSpace => write!(f, "NoSpace"),
        }
    }
}

impl Error for VfsFsError {}