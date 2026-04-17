use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum FFRError {
    NotFound(String),
    PermissionDenied(String),
    BinaryFile(String),
    UnsupportedEncoding(String),
    TooLarge(String),
    InvalidRange(String),
    IOError(String),
    ProtocolError(String),
    SerdeError(String),
    Internal(String),
}

impl Display for FFRError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FFRError::NotFound(msg) => write!(f, "{msg}"),
            FFRError::PermissionDenied(msg) => write!(f, "{msg}"),
            FFRError::BinaryFile(msg) => write!(f, "{msg}"),
            FFRError::UnsupportedEncoding(msg) => write!(f, "{msg}"),
            FFRError::TooLarge(msg) => write!(f, "{msg}"),
            FFRError::InvalidRange(msg) => write!(f, "{msg}"),
            FFRError::IOError(msg) => write!(f, "{msg}"),
            FFRError::ProtocolError(msg) => write!(f, "{msg}"),
            FFRError::SerdeError(msg) => write!(f, "{msg}"),
            FFRError::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FFRError {}

impl From<std::io::Error> for FFRError {
    fn from(err: std::io::Error) -> Self {
        FFRError::IOError(err.to_string())
    }
}

impl From<serde_json::Error> for FFRError {
    fn from(err: serde_json::Error) -> Self {
        FFRError::SerdeError(err.to_string())
    }
}

impl FFRError {
    pub fn code(&self) -> &'static str {
        match self {
            FFRError::NotFound(_) => "NotFound",
            FFRError::PermissionDenied(_) => "PermissionDenied",
            FFRError::BinaryFile(_) => "BinaryFile",
            FFRError::UnsupportedEncoding(_) => "UnsupportedEncoding",
            FFRError::TooLarge(_) => "TooLarge",
            FFRError::InvalidRange(_) => "InvalidRange",
            FFRError::IOError(_) => "IOError",
            FFRError::ProtocolError(_) => "ProtocolError",
            FFRError::SerdeError(_) => "SerdeError",
            FFRError::Internal(_) => "Internal",
        }
    }

    pub fn message(&self) -> String {
        self.to_string()
    }
}