//! Binary settings protocol (delta encoding + zlib compression).

pub mod binary_settings_protocol;

pub use binary_settings_protocol::{
    BinaryMessage, BinarySettingsProtocol, BinaryValue, PathRegistry,
};
