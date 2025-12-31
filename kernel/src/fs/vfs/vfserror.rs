use core::{error::Error, fmt::{Display, Formatter, Result}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsInodeError{
    FileExist,
    DirExist,
    FileNotFound,
    DirNotFound,
    InValidOperate,
    UnSupportOperate,
    OperateFailed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsFsError {
    Mounted,
    Unmounted,
    FsInnerError,
    MountFail,
    UnmountFail,
}

impl Display for VfsInodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::FileExist => write!(f, "FileExist"),
            Self::DirExist => write!(f, "DirExist"),
            Self::FileNotFound => write!(f, "FileNotFound"),
            Self::DirNotFound => write!(f, "DirNotFound"),
            Self::InValidOperate => write!(f, "InValidOperate"),
            Self::UnSupportOperate => write!(f, "UnSupportOperate"),
            Self::OperateFailed => write!(f,"OperateFailed"),
        }
    }
}

impl Error for VfsInodeError {}

impl Display for VfsFsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Mounted => write!(f, "Mounted"),
            Self::Unmounted => write!(f, "Unmounted"),
            Self::FsInnerError => write!(f, "FsInnerError"),
            Self::MountFail => write!(f, "MountFail"),
            Self::UnmountFail => write!(f, "UnmountFail"),
        }
    }
}

impl Error for VfsFsError {}