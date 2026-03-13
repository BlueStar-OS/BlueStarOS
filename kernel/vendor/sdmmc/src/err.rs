// ===== Types and Structures =====

use core::fmt;

#[derive(Debug)]
pub enum SdError {
    /// Command timeout
    Timeout,
    /// Command CRC error
    Crc,
    /// Command end bit error
    EndBit,
    /// Command index error
    Index,
    /// Data timeout
    DataTimeout,
    /// Data CRC error
    DataCrc,
    /// Data end bit error
    DataEndBit,
    /// Bus power error
    BusPower,
    /// Auto CMD12 error
    Acmd12Error,
    /// ADMA error
    AdmaError,
    /// Invalid response received
    InvalidResponse,
    /// No card detected
    NoCard,
    /// Unsupported card type
    UnsupportedCard,
    /// General I/O error
    IoError,
    /// Command execution error
    CommandError,
    /// Data transfer error
    TransferError,
    /// Invalid response type for command
    InvalidResponseType,
    /// Current limit exceeded
    CurrentLimit,
    /// General data error
    DataError,
    /// Tuning procedure failed
    TuningFailed,
    /// Voltage switch failed
    VoltageSwitchFailed,
    /// Bad message format
    BadMessage,
    /// Invalid argument provided
    InvalidArgument,
    /// Buffer overflow occurred
    BufferOverflow,
    /// Memory allocation or access error
    MemoryError,
    /// Bus width configuration error
    BusWidth,
    /// Card-specific error with status and description
    CardError(u32, &'static str),
}

impl fmt::Display for SdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SdError::Timeout => write!(f, "Command timeout error"),
            SdError::Crc => write!(f, "Command CRC error"),
            SdError::EndBit => write!(f, "Command end bit error"),
            SdError::Index => write!(f, "Command index error"),
            SdError::DataTimeout => write!(f, "Data timeout error"),
            SdError::DataCrc => write!(f, "Data CRC error"),
            SdError::DataEndBit => write!(f, "Data end bit error"),
            SdError::BusPower => write!(f, "Bus power error"),
            SdError::Acmd12Error => write!(f, "ACMD12 error"),
            SdError::AdmaError => write!(f, "ADMA error"),
            SdError::InvalidResponse => write!(f, "Invalid response"),
            SdError::NoCard => write!(f, "No card detected"),
            SdError::UnsupportedCard => write!(f, "Unsupported card"),
            SdError::IoError => write!(f, "I/O error"),
            SdError::CommandError => write!(f, "Command error"),
            SdError::TransferError => write!(f, "Transfer error"),
            SdError::InvalidResponseType => write!(f, "Invalid response type"),
            SdError::CurrentLimit => write!(f, "Current limit error"),
            SdError::DataError => write!(f, "Data error"),
            SdError::TuningFailed => write!(f, "Tuning failed"),
            SdError::VoltageSwitchFailed => write!(f, "Voltage switch failed"),
            SdError::BadMessage => write!(f, "Bad message"),
            SdError::InvalidArgument => write!(f, "Invalid argument"),
            SdError::BufferOverflow => write!(f, "Buffer overflow"),
            SdError::MemoryError => write!(f, "Memory error"),
            SdError::BusWidth => write!(f, "Bus width error"),
            SdError::CardError(status, desc) => write!(f, "Card error: 0x{:X} ({})", status, desc),
        }
    }
}
