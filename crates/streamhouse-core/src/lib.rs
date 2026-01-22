pub mod error;
pub mod record;
pub mod segment;
pub mod varint;

pub use error::{Error, Result};
pub use record::Record;
pub use segment::SegmentInfo;
