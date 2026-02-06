//! ApiVersions API handler (API Key 18)
//!
//! Returns the versions of each API supported by the broker.
//! This is typically the first request a client sends.

use bytes::{BufMut, BytesMut};

use crate::codec::{
    encode_compact_string, encode_empty_tagged_fields, encode_unsigned_varint, RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;
use crate::types::supported_api_versions;

/// Handle ApiVersions request
pub async fn handle_api_versions(
    _state: &KafkaServerState,
    header: &RequestHeader,
    _body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    let mut response = BytesMut::new();

    let api_versions = supported_api_versions();

    if header.api_version >= 3 {
        // Compact protocol (v3+)
        response.put_i16(ErrorCode::None.as_i16());

        // Api versions array (compact)
        encode_unsigned_varint(&mut response, (api_versions.len() + 1) as u64);
        for api in &api_versions {
            response.put_i16(api.api_key);
            response.put_i16(api.min_version);
            response.put_i16(api.max_version);
            encode_empty_tagged_fields(&mut response);
        }

        // Throttle time
        response.put_i32(0);

        // Tagged fields
        encode_empty_tagged_fields(&mut response);
    } else {
        // Legacy protocol (v0-v2)
        response.put_i16(ErrorCode::None.as_i16());

        // Api versions array
        response.put_i32(api_versions.len() as i32);
        for api in &api_versions {
            response.put_i16(api.api_key);
            response.put_i16(api.min_version);
            response.put_i16(api.max_version);
        }

        if header.api_version >= 1 {
            // Throttle time (v1+)
            response.put_i32(0);
        }
    }

    Ok(response)
}
