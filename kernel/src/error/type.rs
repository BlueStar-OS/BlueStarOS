//! 错误类型定义，错误常量定义，参考 linux 源码
//! - other/linux-5.4.29/include/uapi/asm-generic/errno-base.h (1-34)
//! - other/linux-5.4.29/include/uapi/asm-generic/errno.h (35-133)

/// Linux 兼容错误码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BlueErr {
    // ===== errno-base.h (1-34) =====
    EPERM = 1,    // Operation not permitted
    ENOENT = 2,   // No such file or directory
    ESRCH = 3,    // No such process
    EINTR = 4,    // Interrupted system call
    EIO = 5,      // I/O error
    ENXIO = 6,    // No such device or address
    E2BIG = 7,    // Argument list too long
    ENOEXEC = 8,  // Exec format error
    EBADF = 9,    // Bad file number
    ECHILD = 10,  // No child processes
    EAGAIN = 11,  // Try again (EWOULDBLOCK)
    ENOMEM = 12,  // Out of memory
    EACCES = 13,  // Permission denied
    EFAULT = 14,  // Bad address
    ENOTBLK = 15, // Block device required
    EBUSY = 16,   // Device or resource busy
    EEXIST = 17,  // File exists
    EXDEV = 18,   // Cross-device link
    ENODEV = 19,  // No such device
    ENOTDIR = 20, // Not a directory
    EISDIR = 21,  // Is a directory
    EINVAL = 22,  // Invalid argument
    ENFILE = 23,  // File table overflow
    EMFILE = 24,  // Too many open files
    ENOTTY = 25,  // Not a typewriter
    ETXTBSY = 26, // Text file busy
    EFBIG = 27,   // File too large
    ENOSPC = 28,  // No space left on device
    ESPIPE = 29,  // Illegal seek
    EROFS = 30,   // Read-only file system
    EMLINK = 31,  // Too many links
    EPIPE = 32,   // Broken pipe
    EDOM = 33,    // Math argument out of domain
    ERANGE = 34,  // Math result not representable

    // ===== errno.h (35-133) =====
    EDEADLK = 35,      // Resource deadlock would occur
    ENAMETOOLONG = 36, // File name too long
    ENOLCK = 37,       // No record locks available
    ENOSYS = 38,       // Invalid system call number
    ENOTEMPTY = 39,    // Directory not empty
    ELOOP = 40,        // Too many symbolic links
    // 41 is EWOULDBLOCK (alias for EAGAIN)
    ENOMSG = 42,   // No message of desired type
    EIDRM = 43,    // Identifier removed
    ECHRNG = 44,   // Channel number out of range
    EL2NSYNC = 45, // Level 2 not synchronized
    EL3HLT = 46,   // Level 3 halted
    EL3RST = 47,   // Level 3 reset
    ELNRNG = 48,   // Link number out of range
    EUNATCH = 49,  // Protocol driver not attached
    ENOCSI = 50,   // No CSI structure available
    EL2HLT = 51,   // Level 2 halted
    EBADE = 52,    // Invalid exchange
    EBADR = 53,    // Invalid request descriptor
    EXFULL = 54,   // Exchange full
    ENOANO = 55,   // No anode
    EBADRQC = 56,  // Invalid request code
    EBADSLT = 57,  // Invalid slot
    // 58 is EDEADLOCK (alias for EDEADLK)
    EBFONT = 59,           // Bad font file format
    ENOSTR = 60,           // Device not a stream
    ENODATA = 61,          // No data available
    ETIME = 62,            // Timer expired
    ENOSR = 63,            // Out of streams resources
    ENONET = 64,           // Machine is not on the network
    ENOPKG = 65,           // Package not installed
    EREMOTE = 66,          // Object is remote
    ENOLINK = 67,          // Link has been severed
    EADV = 68,             // Advertise error
    ESRMNT = 69,           // Srmount error
    ECOMM = 70,            // Communication error on send
    EPROTO = 71,           // Protocol error
    EMULTIHOP = 72,        // Multihop attempted
    EDOTDOT = 73,          // RFS specific error
    EBADMSG = 74,          // Not a data message
    EOVERFLOW = 75,        // Value too large for defined data type
    ENOTUNIQ = 76,         // Name not unique on network
    EBADFD = 77,           // File descriptor in bad state
    EREMCHG = 78,          // Remote address changed
    ELIBACC = 79,          // Can not access a needed shared library
    ELIBBAD = 80,          // Accessing a corrupted shared library
    ELIBSCN = 81,          // .lib section in a.out corrupted
    ELIBMAX = 82,          // Attempting to link in too many shared libraries
    ELIBEXEC = 83,         // Cannot exec a shared library directly
    EILSEQ = 84,           // Illegal byte sequence
    ERESTART = 85,         // Interrupted system call should be restarted
    ESTRPIPE = 86,         // Streams pipe error
    EUSERS = 87,           // Too many users
    ENOTSOCK = 88,         // Socket operation on non-socket
    EDESTADDRREQ = 89,     // Destination address required
    EMSGSIZE = 90,         // Message too long
    EPROTOTYPE = 91,       // Protocol wrong type for socket
    ENOPROTOOPT = 92,      // Protocol not available
    EPROTONOSUPPORT = 93,  // Protocol not supported
    ESOCKTNOSUPPORT = 94,  // Socket type not supported
    EOPNOTSUPP = 95,       // Operation not supported on transport endpoint
    EPFNOSUPPORT = 96,     // Protocol family not supported
    EAFNOSUPPORT = 97,     // Address family not supported by protocol
    EADDRINUSE = 98,       // Address already in use
    EADDRNOTAVAIL = 99,    // Cannot assign requested address
    ENETDOWN = 100,        // Network is down
    ENETUNREACH = 101,     // Network is unreachable
    ENETRESET = 102,       // Network dropped connection because of reset
    ECONNABORTED = 103,    // Software caused connection abort
    ECONNRESET = 104,      // Connection reset by peer
    ENOBUFS = 105,         // No buffer space available
    EISCONN = 106,         // Transport endpoint is already connected
    ENOTCONN = 107,        // Transport endpoint is not connected
    ESHUTDOWN = 108,       // Cannot send after transport endpoint shutdown
    ETOOMANYREFS = 109,    // Too many references: cannot splice
    ETIMEDOUT = 110,       // Connection timed out
    ECONNREFUSED = 111,    // Connection refused
    EHOSTDOWN = 112,       // Host is down
    EHOSTUNREACH = 113,    // No route to host
    EALREADY = 114,        // Operation already in progress
    EINPROGRESS = 115,     // Operation now in progress
    ESTALE = 116,          // Stale file handle
    EUCLEAN = 117,         // Structure needs cleaning
    ENOTNAM = 118,         // Not a XENIX named type file
    ENAVAIL = 119,         // No XENIX semaphores available
    EISNAM = 120,          // Is a named type file
    EREMOTEIO = 121,       // Remote I/O error
    EDQUOT = 122,          // Quota exceeded
    ENOMEDIUM = 123,       // No medium found
    EMEDIUMTYPE = 124,     // Wrong medium type
    ECANCELED = 125,       // Operation Canceled
    ENOKEY = 126,          // Required key not available
    EKEYEXPIRED = 127,     // Key has expired
    EKEYREVOKED = 128,     // Key has been revoked
    EKEYREJECTED = 129,    // Key was rejected by service
    EOWNERDEAD = 130,      // Owner died
    ENOTRECOVERABLE = 131, // State not recoverable
    ERFKILL = 132,         // Operation not possible due to RF-kill
    EHWPOISON = 133,       // Memory page has hardware error
}

// 别名常量
pub const EWOULDBLOCK: BlueErr = BlueErr::EAGAIN;
pub const EDEADLOCK: BlueErr = BlueErr::EDEADLK;

impl BlueErr {
    /// 返回正数错误码
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// 返回 syscall 使用的负数错误码
    pub const fn as_isize(self) -> isize {
        -(self as isize)
    }

    /// 转换为 syscall 错误返回值（负数 errno），用于 syscall handler 直接返回
    pub const fn to_syscall_ret(self) -> isize {
        -(self as isize)
    }

    /// 返回错误描述
    pub const fn message(self) -> &'static str {
        match self {
            BlueErr::EPERM => "Operation not permitted",
            BlueErr::ENOENT => "No such file or directory",
            BlueErr::ESRCH => "No such process",
            BlueErr::EINTR => "Interrupted system call",
            BlueErr::EIO => "I/O error",
            BlueErr::ENXIO => "No such device or address",
            BlueErr::E2BIG => "Argument list too long",
            BlueErr::ENOEXEC => "Exec format error",
            BlueErr::EBADF => "Bad file number",
            BlueErr::ECHILD => "No child processes",
            BlueErr::EAGAIN => "Try again",
            BlueErr::ENOMEM => "Out of memory",
            BlueErr::EACCES => "Permission denied",
            BlueErr::EFAULT => "Bad address",
            BlueErr::ENOTBLK => "Block device required",
            BlueErr::EBUSY => "Device or resource busy",
            BlueErr::EEXIST => "File exists",
            BlueErr::EXDEV => "Cross-device link",
            BlueErr::ENODEV => "No such device",
            BlueErr::ENOTDIR => "Not a directory",
            BlueErr::EISDIR => "Is a directory",
            BlueErr::EINVAL => "Invalid argument",
            BlueErr::ENFILE => "File table overflow",
            BlueErr::EMFILE => "Too many open files",
            BlueErr::ENOTTY => "Not a typewriter",
            BlueErr::ETXTBSY => "Text file busy",
            BlueErr::EFBIG => "File too large",
            BlueErr::ENOSPC => "No space left on device",
            BlueErr::ESPIPE => "Illegal seek",
            BlueErr::EROFS => "Read-only file system",
            BlueErr::EMLINK => "Too many links",
            BlueErr::EPIPE => "Broken pipe",
            BlueErr::EDOM => "Math argument out of domain",
            BlueErr::ERANGE => "Math result not representable",
            BlueErr::EDEADLK => "Resource deadlock would occur",
            BlueErr::ENAMETOOLONG => "File name too long",
            BlueErr::ENOLCK => "No record locks available",
            BlueErr::ENOSYS => "Invalid system call number",
            BlueErr::ENOTEMPTY => "Directory not empty",
            BlueErr::ELOOP => "Too many symbolic links",
            BlueErr::ENOMSG => "No message of desired type",
            BlueErr::EIDRM => "Identifier removed",
            BlueErr::ECHRNG => "Channel number out of range",
            BlueErr::EL2NSYNC => "Level 2 not synchronized",
            BlueErr::EL3HLT => "Level 3 halted",
            BlueErr::EL3RST => "Level 3 reset",
            BlueErr::ELNRNG => "Link number out of range",
            BlueErr::EUNATCH => "Protocol driver not attached",
            BlueErr::ENOCSI => "No CSI structure available",
            BlueErr::EL2HLT => "Level 2 halted",
            BlueErr::EBADE => "Invalid exchange",
            BlueErr::EBADR => "Invalid request descriptor",
            BlueErr::EXFULL => "Exchange full",
            BlueErr::ENOANO => "No anode",
            BlueErr::EBADRQC => "Invalid request code",
            BlueErr::EBADSLT => "Invalid slot",
            BlueErr::EBFONT => "Bad font file format",
            BlueErr::ENOSTR => "Device not a stream",
            BlueErr::ENODATA => "No data available",
            BlueErr::ETIME => "Timer expired",
            BlueErr::ENOSR => "Out of streams resources",
            BlueErr::ENONET => "Machine is not on the network",
            BlueErr::ENOPKG => "Package not installed",
            BlueErr::EREMOTE => "Object is remote",
            BlueErr::ENOLINK => "Link has been severed",
            BlueErr::EADV => "Advertise error",
            BlueErr::ESRMNT => "Srmount error",
            BlueErr::ECOMM => "Communication error on send",
            BlueErr::EPROTO => "Protocol error",
            BlueErr::EMULTIHOP => "Multihop attempted",
            BlueErr::EDOTDOT => "RFS specific error",
            BlueErr::EBADMSG => "Not a data message",
            BlueErr::EOVERFLOW => "Value too large for defined data type",
            BlueErr::ENOTUNIQ => "Name not unique on network",
            BlueErr::EBADFD => "File descriptor in bad state",
            BlueErr::EREMCHG => "Remote address changed",
            BlueErr::ELIBACC => "Can not access a needed shared library",
            BlueErr::ELIBBAD => "Accessing a corrupted shared library",
            BlueErr::ELIBSCN => ".lib section in a.out corrupted",
            BlueErr::ELIBMAX => "Attempting to link in too many shared libraries",
            BlueErr::ELIBEXEC => "Cannot exec a shared library directly",
            BlueErr::EILSEQ => "Illegal byte sequence",
            BlueErr::ERESTART => "Interrupted system call should be restarted",
            BlueErr::ESTRPIPE => "Streams pipe error",
            BlueErr::EUSERS => "Too many users",
            BlueErr::ENOTSOCK => "Socket operation on non-socket",
            BlueErr::EDESTADDRREQ => "Destination address required",
            BlueErr::EMSGSIZE => "Message too long",
            BlueErr::EPROTOTYPE => "Protocol wrong type for socket",
            BlueErr::ENOPROTOOPT => "Protocol not available",
            BlueErr::EPROTONOSUPPORT => "Protocol not supported",
            BlueErr::ESOCKTNOSUPPORT => "Socket type not supported",
            BlueErr::EOPNOTSUPP => "Operation not supported on transport endpoint",
            BlueErr::EPFNOSUPPORT => "Protocol family not supported",
            BlueErr::EAFNOSUPPORT => "Address family not supported by protocol",
            BlueErr::EADDRINUSE => "Address already in use",
            BlueErr::EADDRNOTAVAIL => "Cannot assign requested address",
            BlueErr::ENETDOWN => "Network is down",
            BlueErr::ENETUNREACH => "Network is unreachable",
            BlueErr::ENETRESET => "Network dropped connection because of reset",
            BlueErr::ECONNABORTED => "Software caused connection abort",
            BlueErr::ECONNRESET => "Connection reset by peer",
            BlueErr::ENOBUFS => "No buffer space available",
            BlueErr::EISCONN => "Transport endpoint is already connected",
            BlueErr::ENOTCONN => "Transport endpoint is not connected",
            BlueErr::ESHUTDOWN => "Cannot send after transport endpoint shutdown",
            BlueErr::ETOOMANYREFS => "Too many references: cannot splice",
            BlueErr::ETIMEDOUT => "Connection timed out",
            BlueErr::ECONNREFUSED => "Connection refused",
            BlueErr::EHOSTDOWN => "Host is down",
            BlueErr::EHOSTUNREACH => "No route to host",
            BlueErr::EALREADY => "Operation already in progress",
            BlueErr::EINPROGRESS => "Operation now in progress",
            BlueErr::ESTALE => "Stale file handle",
            BlueErr::EUCLEAN => "Structure needs cleaning",
            BlueErr::ENOTNAM => "Not a XENIX named type file",
            BlueErr::ENAVAIL => "No XENIX semaphores available",
            BlueErr::EISNAM => "Is a named type file",
            BlueErr::EREMOTEIO => "Remote I/O error",
            BlueErr::EDQUOT => "Quota exceeded",
            BlueErr::ENOMEDIUM => "No medium found",
            BlueErr::EMEDIUMTYPE => "Wrong medium type",
            BlueErr::ECANCELED => "Operation Canceled",
            BlueErr::ENOKEY => "Required key not available",
            BlueErr::EKEYEXPIRED => "Key has expired",
            BlueErr::EKEYREVOKED => "Key has been revoked",
            BlueErr::EKEYREJECTED => "Key was rejected by service",
            BlueErr::EOWNERDEAD => "Owner died",
            BlueErr::ENOTRECOVERABLE => "State not recoverable",
            BlueErr::ERFKILL => "Operation not possible due to RF-kill",
            BlueErr::EHWPOISON => "Memory page has hardware error",
        }
    }
}

impl core::fmt::Display for BlueErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl core::ops::Neg for BlueErr {
    type Output = isize;

    fn neg(self) -> Self::Output {
        -(self as isize)
    }
}
