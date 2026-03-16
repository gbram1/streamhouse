//! SASL authentication handlers (API Keys 17 + 36)
//!
//! Implements SaslHandshake and SaslAuthenticate for SASL/PLAIN authentication.

use bytes::{Buf, BufMut, BytesMut};

use crate::codec::{
    encode_compact_nullable_bytes, encode_compact_nullable_string, encode_empty_tagged_fields,
    encode_nullable_string, encode_string, parse_string, RequestHeader,
};
use crate::error::{ErrorCode, KafkaResult};
use crate::server::KafkaServerState;
use crate::tenant::KafkaTenantContext;

/// Handle SaslHandshake request (API Key 17).
///
/// Returns supported SASL mechanisms. We only support PLAIN.
pub async fn handle_sasl_handshake(
    _state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<BytesMut> {
    // Parse the requested mechanism
    let mechanism = if header.api_version >= 1 {
        // v1 uses non-flexible encoding (SaslHandshake is never flexible)
        parse_string(body)?
    } else {
        parse_string(body)?
    };

    let supported = mechanism.eq_ignore_ascii_case("PLAIN");

    let mut response = BytesMut::new();

    if header.api_version >= 1 {
        // v1 response (still legacy encoding — SaslHandshake is never flexible)
        let error_code = if supported {
            ErrorCode::None
        } else {
            ErrorCode::UnsupportedSaslMechanism
        };
        response.put_i16(error_code.as_i16());

        // mechanisms array (int32 count + strings)
        response.put_i32(1);
        encode_string(&mut response, "PLAIN");
    } else {
        // v0 response
        let error_code = if supported {
            ErrorCode::None
        } else {
            ErrorCode::UnsupportedSaslMechanism
        };
        response.put_i16(error_code.as_i16());

        // mechanisms array
        response.put_i32(1);
        encode_string(&mut response, "PLAIN");
    }

    Ok(response)
}

/// Handle SaslAuthenticate request (API Key 36).
///
/// Decodes SASL/PLAIN credentials and resolves the tenant.
/// Returns (response_bytes, optional_tenant_context).
pub async fn handle_sasl_authenticate(
    state: &KafkaServerState,
    header: &RequestHeader,
    body: &mut BytesMut,
) -> KafkaResult<(BytesMut, Option<KafkaTenantContext>)> {
    // Parse auth_bytes from request
    let auth_bytes = if header.api_version >= 2 {
        // v2 flexible: compact bytes
        let len = crate::codec::parse_unsigned_varint(body)? as usize;
        if len == 0 {
            vec![]
        } else {
            let actual_len = len - 1;
            if body.len() < actual_len {
                return Ok((build_sasl_auth_error(header, "Invalid auth data"), None));
            }
            body.split_to(actual_len).to_vec()
        }
    } else {
        // v0-v1: int32 length + bytes
        if body.len() < 4 {
            return Ok((build_sasl_auth_error(header, "Invalid auth data"), None));
        }
        let len = body.get_i32();
        if len < 0 || body.len() < len as usize {
            return Ok((build_sasl_auth_error(header, "Invalid auth data"), None));
        }
        body.split_to(len as usize).to_vec()
    };

    // Skip tagged fields for v2+
    if header.api_version >= 2 {
        let _ = crate::codec::skip_tagged_fields(body);
    }

    // Decode SASL/PLAIN: \0username\0password
    let (username, password) = match decode_sasl_plain(&auth_bytes) {
        Some(creds) => creds,
        None => {
            return Ok((
                build_sasl_auth_error(header, "Invalid SASL/PLAIN format"),
                None,
            ));
        }
    };

    // Resolve tenant via the resolver
    let resolver = match &state.tenant_resolver {
        Some(r) => r,
        None => {
            return Ok((
                build_sasl_auth_error(header, "Authentication not configured"),
                None,
            ));
        }
    };

    match resolver.resolve_sasl_plain(&username, &password).await {
        Ok(ctx) => {
            let mut response = BytesMut::new();
            if header.api_version >= 2 {
                // v2 flexible
                response.put_i16(ErrorCode::None.as_i16());
                encode_compact_nullable_string(&mut response, None); // error_message
                encode_compact_nullable_bytes(&mut response, None); // auth_bytes
                if header.api_version >= 1 {
                    response.put_i64(0); // session_lifetime_ms
                }
                encode_empty_tagged_fields(&mut response);
            } else if header.api_version == 1 {
                // v1 legacy
                response.put_i16(ErrorCode::None.as_i16());
                encode_nullable_string(&mut response, None); // error_message
                response.put_i32(-1); // auth_bytes (null)
                response.put_i64(0); // session_lifetime_ms
            } else {
                // v0 legacy
                response.put_i16(ErrorCode::None.as_i16());
                encode_nullable_string(&mut response, None); // error_message
                response.put_i32(-1); // auth_bytes (null)
            }
            Ok((response, Some(ctx)))
        }
        Err(e) => {
            let msg = format!("Authentication failed: {}", e);
            Ok((build_sasl_auth_error(header, &msg), None))
        }
    }
}

/// Build a SaslAuthenticate error response.
fn build_sasl_auth_error(header: &RequestHeader, message: &str) -> BytesMut {
    let mut response = BytesMut::new();
    if header.api_version >= 2 {
        // v2 flexible
        response.put_i16(ErrorCode::SaslAuthenticationFailed.as_i16());
        encode_compact_nullable_string(&mut response, Some(message));
        encode_compact_nullable_bytes(&mut response, None); // auth_bytes
        if header.api_version >= 1 {
            response.put_i64(0); // session_lifetime_ms
        }
        encode_empty_tagged_fields(&mut response);
    } else if header.api_version == 1 {
        // v1
        response.put_i16(ErrorCode::SaslAuthenticationFailed.as_i16());
        encode_nullable_string(&mut response, Some(message));
        response.put_i32(-1); // auth_bytes
        response.put_i64(0); // session_lifetime_ms
    } else {
        // v0
        response.put_i16(ErrorCode::SaslAuthenticationFailed.as_i16());
        encode_nullable_string(&mut response, Some(message));
        response.put_i32(-1); // auth_bytes
    }
    response
}

/// Decode SASL/PLAIN authentication data.
///
/// Format: `\0username\0password`
/// Returns (username, password) on success.
fn decode_sasl_plain(data: &[u8]) -> Option<(String, String)> {
    // SASL/PLAIN format: [authzid]\0authcid\0passwd
    // authzid is typically empty, so format is: \0username\0password
    let mut parts = data.splitn(3, |&b| b == 0);

    // Skip authzid (first part, typically empty)
    let _authzid = parts.next()?;

    let username = parts.next()?;
    let password = parts.next()?;

    let username = String::from_utf8(username.to_vec()).ok()?;
    let password = String::from_utf8(password.to_vec()).ok()?;

    if username.is_empty() {
        return None;
    }

    Some((username, password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_sasl_plain_valid() {
        // \0username\0password
        let data = b"\0myuser\0mypass";
        let (user, pass) = decode_sasl_plain(data).unwrap();
        assert_eq!(user, "myuser");
        assert_eq!(pass, "mypass");
    }

    #[test]
    fn test_decode_sasl_plain_with_authzid() {
        // authzid\0username\0password
        let data = b"authzid\0myuser\0mypass";
        let (user, pass) = decode_sasl_plain(data).unwrap();
        assert_eq!(user, "myuser");
        assert_eq!(pass, "mypass");
    }

    #[test]
    fn test_decode_sasl_plain_empty_username() {
        let data = b"\0\0mypass";
        assert!(decode_sasl_plain(data).is_none());
    }

    #[test]
    fn test_decode_sasl_plain_no_separator() {
        let data = b"noseparator";
        assert!(decode_sasl_plain(data).is_none());
    }

    #[test]
    fn test_decode_sasl_plain_one_separator() {
        let data = b"\0onlyuser";
        assert!(decode_sasl_plain(data).is_none());
    }

    #[test]
    fn test_decode_sasl_plain_api_key_style() {
        // Real usage: \0sk_live_abc123\0sk_live_abc123
        let key = "sk_live_abc123def456";
        let mut data = Vec::new();
        data.push(0);
        data.extend_from_slice(key.as_bytes());
        data.push(0);
        data.extend_from_slice(key.as_bytes());

        let (user, pass) = decode_sasl_plain(&data).unwrap();
        assert_eq!(user, key);
        assert_eq!(pass, key);
    }
}
