//! SASL/SCRAM Authentication
//!
//! Implements SCRAM (Salted Challenge Response Authentication Mechanism) for
//! Kafka-compatible client authentication.
//!
//! ## Supported Mechanisms
//!
//! - SCRAM-SHA-256 (recommended)
//! - SCRAM-SHA-512 (stronger, more CPU intensive)
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_api::sasl::{ScramServer, ScramConfig, ScramMechanism};
//!
//! // Server-side setup
//! let config = ScramConfig::default();
//! let server = ScramServer::new(config);
//!
//! // Store user credentials
//! server.store_credentials("user1", "password123").await?;
//!
//! // During authentication:
//! // 1. Client sends client-first message
//! let server_first = server.handle_client_first(&client_first)?;
//! // 2. Server responds with server-first
//! // 3. Client sends client-final message
//! let (server_final, authenticated) = server.handle_client_final(&client_final, &state)?;
//! ```
//!
//! ## Wire Protocol
//!
//! SCRAM follows RFC 5802 for the authentication exchange:
//!
//! 1. Client → Server: client-first-message
//!    - Contains: username, nonce, optional extensions
//!
//! 2. Server → Client: server-first-message
//!    - Contains: server nonce, salt, iteration count
//!
//! 3. Client → Server: client-final-message
//!    - Contains: channel binding, combined nonce, proof
//!
//! 4. Server → Client: server-final-message
//!    - Contains: server signature (for mutual auth)

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// SASL/SCRAM errors
#[derive(Debug, Error)]
pub enum ScramError {
    #[error("Invalid mechanism: {0}")]
    InvalidMechanism(String),

    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Invalid state")]
    InvalidState,

    #[error("Nonce mismatch")]
    NonceMismatch,

    #[error("Server signature mismatch")]
    ServerSignatureMismatch,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ScramError>;

/// SCRAM mechanism types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScramMechanism {
    /// SCRAM-SHA-256 (recommended)
    ScramSha256,
    /// SCRAM-SHA-512 (stronger)
    ScramSha512,
}

impl ScramMechanism {
    /// Parse mechanism from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "SCRAM-SHA-256" => Ok(ScramMechanism::ScramSha256),
            "SCRAM-SHA-512" => Ok(ScramMechanism::ScramSha512),
            _ => Err(ScramError::InvalidMechanism(s.to_string())),
        }
    }

    /// Get mechanism name
    pub fn name(&self) -> &'static str {
        match self {
            ScramMechanism::ScramSha256 => "SCRAM-SHA-256",
            ScramMechanism::ScramSha512 => "SCRAM-SHA-512",
        }
    }

    /// Get the hash output length
    pub fn hash_length(&self) -> usize {
        match self {
            ScramMechanism::ScramSha256 => 32,
            ScramMechanism::ScramSha512 => 64,
        }
    }
}

/// SCRAM server configuration
#[derive(Debug, Clone)]
pub struct ScramConfig {
    /// Supported mechanisms
    pub mechanisms: Vec<ScramMechanism>,
    /// PBKDF2 iteration count
    pub iteration_count: u32,
    /// Salt length in bytes
    pub salt_length: usize,
    /// Nonce length in bytes
    pub nonce_length: usize,
}

impl Default for ScramConfig {
    fn default() -> Self {
        Self {
            mechanisms: vec![ScramMechanism::ScramSha256, ScramMechanism::ScramSha512],
            iteration_count: 4096,
            salt_length: 16,
            nonce_length: 24,
        }
    }
}

impl ScramConfig {
    /// Create config with only SHA-256
    pub fn sha256_only() -> Self {
        Self {
            mechanisms: vec![ScramMechanism::ScramSha256],
            ..Default::default()
        }
    }

    /// Create config with only SHA-512
    pub fn sha512_only() -> Self {
        Self {
            mechanisms: vec![ScramMechanism::ScramSha512],
            ..Default::default()
        }
    }

    /// Set iteration count
    pub fn iteration_count(mut self, count: u32) -> Self {
        self.iteration_count = count;
        self
    }

    /// Check if mechanism is supported
    pub fn supports(&self, mechanism: ScramMechanism) -> bool {
        self.mechanisms.contains(&mechanism)
    }
}

/// Stored user credentials (derived keys, not plaintext password)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredentials {
    /// Username
    pub username: String,
    /// Salt (base64 encoded)
    pub salt: String,
    /// Iteration count used
    pub iteration_count: u32,
    /// Stored key (base64 encoded)
    pub stored_key: String,
    /// Server key (base64 encoded)
    pub server_key: String,
    /// Mechanism used
    pub mechanism: ScramMechanism,
}

/// Authentication state during SCRAM exchange
#[derive(Debug, Clone)]
pub struct ScramState {
    /// Username being authenticated
    pub username: String,
    /// Client nonce
    pub client_nonce: String,
    /// Server nonce
    pub server_nonce: String,
    /// Combined nonce (client + server)
    pub combined_nonce: String,
    /// Salt (base64 encoded)
    pub salt: String,
    /// Iteration count
    pub iteration_count: u32,
    /// Auth message for signature computation
    pub auth_message: String,
    /// Stored key (for verification)
    pub stored_key: Vec<u8>,
    /// Server key (for server signature)
    pub server_key: Vec<u8>,
    /// Mechanism
    pub mechanism: ScramMechanism,
}

/// Parsed client-first message
#[derive(Debug, Clone)]
pub struct ClientFirstMessage {
    /// GS2 header (channel binding info)
    pub gs2_header: String,
    /// Username
    pub username: String,
    /// Client nonce
    pub nonce: String,
    /// Optional extensions
    pub extensions: HashMap<String, String>,
    /// Raw message without GS2 header (for auth message)
    pub bare: String,
}

/// Parsed client-final message
#[derive(Debug, Clone)]
pub struct ClientFinalMessage {
    /// Channel binding data
    pub channel_binding: String,
    /// Combined nonce
    pub nonce: String,
    /// Client proof (base64)
    pub proof: String,
    /// Raw message without proof (for auth message)
    pub without_proof: String,
}

impl ClientFirstMessage {
    /// Parse client-first-message
    pub fn parse(message: &str) -> Result<Self> {
        // Format: gs2-header,username=user,r=nonce[,extensions]
        // GS2 header is: n,, or y,, or p=<binding-name>,,

        let parts: Vec<&str> = message.splitn(3, ',').collect();
        if parts.len() < 3 {
            return Err(ScramError::InvalidFormat(
                "Client-first message must have at least 3 comma-separated parts".to_string(),
            ));
        }

        // GS2 header is first two parts plus comma
        let gs2_header = format!("{},{},", parts[0], parts[1]);

        // Rest is the bare message
        let bare = parts[2].to_string();

        // Parse attributes from bare message
        let mut username = None;
        let mut nonce = None;
        let mut extensions = HashMap::new();

        for attr in bare.split(',') {
            if attr.is_empty() {
                continue;
            }

            let (key, value) = attr
                .split_once('=')
                .ok_or_else(|| ScramError::InvalidFormat(format!("Invalid attribute: {}", attr)))?;

            match key {
                "n" => username = Some(value.to_string()),
                "r" => nonce = Some(value.to_string()),
                _ => {
                    extensions.insert(key.to_string(), value.to_string());
                }
            }
        }

        Ok(Self {
            gs2_header,
            username: username.ok_or_else(|| {
                ScramError::InvalidFormat("Missing username (n=) in client-first".to_string())
            })?,
            nonce: nonce.ok_or_else(|| {
                ScramError::InvalidFormat("Missing nonce (r=) in client-first".to_string())
            })?,
            extensions,
            bare,
        })
    }
}

impl ClientFinalMessage {
    /// Parse client-final-message
    pub fn parse(message: &str) -> Result<Self> {
        // Format: c=channel-binding,r=nonce,p=proof

        let mut channel_binding = None;
        let mut nonce = None;
        let mut proof = None;
        let mut without_proof_parts = Vec::new();

        for attr in message.split(',') {
            let (key, value) = attr
                .split_once('=')
                .ok_or_else(|| ScramError::InvalidFormat(format!("Invalid attribute: {}", attr)))?;

            match key {
                "c" => {
                    channel_binding = Some(value.to_string());
                    without_proof_parts.push(attr.to_string());
                }
                "r" => {
                    nonce = Some(value.to_string());
                    without_proof_parts.push(attr.to_string());
                }
                "p" => {
                    proof = Some(value.to_string());
                }
                _ => {
                    without_proof_parts.push(attr.to_string());
                }
            }
        }

        Ok(Self {
            channel_binding: channel_binding.ok_or_else(|| {
                ScramError::InvalidFormat(
                    "Missing channel binding (c=) in client-final".to_string(),
                )
            })?,
            nonce: nonce.ok_or_else(|| {
                ScramError::InvalidFormat("Missing nonce (r=) in client-final".to_string())
            })?,
            proof: proof.ok_or_else(|| {
                ScramError::InvalidFormat("Missing proof (p=) in client-final".to_string())
            })?,
            without_proof: without_proof_parts.join(","),
        })
    }
}

/// SCRAM server for handling authentication
pub struct ScramServer {
    config: ScramConfig,
    /// Stored credentials by username
    credentials: RwLock<HashMap<String, StoredCredentials>>,
    /// Active authentication states by session ID
    states: RwLock<HashMap<String, ScramState>>,
}

impl ScramServer {
    /// Create a new SCRAM server
    pub fn new(config: ScramConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            credentials: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
        })
    }

    /// Store credentials for a user
    pub async fn store_credentials(
        &self,
        username: &str,
        password: &str,
        mechanism: ScramMechanism,
    ) -> Result<()> {
        if !self.config.supports(mechanism) {
            return Err(ScramError::InvalidMechanism(mechanism.name().to_string()));
        }

        // Generate salt
        let mut salt = vec![0u8; self.config.salt_length];
        getrandom::getrandom(&mut salt).map_err(|e| ScramError::Internal(e.to_string()))?;

        // Derive keys
        let (stored_key, server_key) =
            self.derive_keys(password, &salt, self.config.iteration_count, mechanism);

        let creds = StoredCredentials {
            username: username.to_string(),
            salt: STANDARD.encode(&salt),
            iteration_count: self.config.iteration_count,
            stored_key: STANDARD.encode(&stored_key),
            server_key: STANDARD.encode(&server_key),
            mechanism,
        };

        self.credentials
            .write()
            .await
            .insert(username.to_string(), creds);

        Ok(())
    }

    /// Get stored credentials for a user
    pub async fn get_credentials(&self, username: &str) -> Option<StoredCredentials> {
        self.credentials.read().await.get(username).cloned()
    }

    /// Delete credentials for a user
    pub async fn delete_credentials(&self, username: &str) -> bool {
        self.credentials.write().await.remove(username).is_some()
    }

    /// Derive stored key and server key from password
    fn derive_keys(
        &self,
        password: &str,
        salt: &[u8],
        iterations: u32,
        mechanism: ScramMechanism,
    ) -> (Vec<u8>, Vec<u8>) {
        match mechanism {
            ScramMechanism::ScramSha256 => self.derive_keys_sha256(password, salt, iterations),
            ScramMechanism::ScramSha512 => self.derive_keys_sha512(password, salt, iterations),
        }
    }

    fn derive_keys_sha256(
        &self,
        password: &str,
        salt: &[u8],
        iterations: u32,
    ) -> (Vec<u8>, Vec<u8>) {
        // SaltedPassword = Hi(password, salt, i)
        let mut salted_password = vec![0u8; 32];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut salted_password);

        // ClientKey = HMAC(SaltedPassword, "Client Key")
        let mut client_key_hmac = Hmac::<Sha256>::new_from_slice(&salted_password).unwrap();
        client_key_hmac.update(b"Client Key");
        let client_key = client_key_hmac.finalize().into_bytes().to_vec();

        // StoredKey = H(ClientKey)
        let stored_key = Sha256::digest(&client_key).to_vec();

        // ServerKey = HMAC(SaltedPassword, "Server Key")
        let mut server_key_hmac = Hmac::<Sha256>::new_from_slice(&salted_password).unwrap();
        server_key_hmac.update(b"Server Key");
        let server_key = server_key_hmac.finalize().into_bytes().to_vec();

        (stored_key, server_key)
    }

    fn derive_keys_sha512(
        &self,
        password: &str,
        salt: &[u8],
        iterations: u32,
    ) -> (Vec<u8>, Vec<u8>) {
        // SaltedPassword = Hi(password, salt, i)
        let mut salted_password = vec![0u8; 64];
        pbkdf2_hmac::<Sha512>(password.as_bytes(), salt, iterations, &mut salted_password);

        // ClientKey = HMAC(SaltedPassword, "Client Key")
        let mut client_key_hmac = Hmac::<Sha512>::new_from_slice(&salted_password).unwrap();
        client_key_hmac.update(b"Client Key");
        let client_key = client_key_hmac.finalize().into_bytes().to_vec();

        // StoredKey = H(ClientKey)
        let stored_key = Sha512::digest(&client_key).to_vec();

        // ServerKey = HMAC(SaltedPassword, "Server Key")
        let mut server_key_hmac = Hmac::<Sha512>::new_from_slice(&salted_password).unwrap();
        server_key_hmac.update(b"Server Key");
        let server_key = server_key_hmac.finalize().into_bytes().to_vec();

        (stored_key, server_key)
    }

    /// Generate a random nonce
    fn generate_nonce(&self) -> String {
        let mut nonce = vec![0u8; self.config.nonce_length];
        getrandom::getrandom(&mut nonce).expect("Failed to generate random bytes");
        STANDARD.encode(&nonce)
    }

    /// Handle client-first-message, return server-first-message and session ID
    pub async fn handle_client_first(
        &self,
        message: &str,
        mechanism: ScramMechanism,
    ) -> Result<(String, String)> {
        if !self.config.supports(mechanism) {
            return Err(ScramError::InvalidMechanism(mechanism.name().to_string()));
        }

        // Parse client message
        let client_first = ClientFirstMessage::parse(message)?;

        // Look up user credentials
        let creds = self
            .get_credentials(&client_first.username)
            .await
            .ok_or_else(|| ScramError::UserNotFound(client_first.username.clone()))?;

        // Verify mechanism matches stored credentials
        if creds.mechanism != mechanism {
            return Err(ScramError::InvalidMechanism(format!(
                "User {} requires {}",
                client_first.username,
                creds.mechanism.name()
            )));
        }

        // Generate server nonce
        let server_nonce = self.generate_nonce();
        let combined_nonce = format!("{}{}", client_first.nonce, server_nonce);

        // Create server-first-message
        let server_first = format!(
            "r={},s={},i={}",
            combined_nonce, creds.salt, creds.iteration_count
        );

        // Generate session ID
        let mut session_bytes = [0u8; 16];
        getrandom::getrandom(&mut session_bytes).expect("Failed to generate random bytes");
        let session_id = STANDARD.encode(session_bytes);

        // Decode stored keys
        let stored_key = STANDARD
            .decode(&creds.stored_key)
            .map_err(|e| ScramError::Internal(e.to_string()))?;
        let server_key = STANDARD
            .decode(&creds.server_key)
            .map_err(|e| ScramError::Internal(e.to_string()))?;

        // Store state
        let state = ScramState {
            username: client_first.username,
            client_nonce: client_first.nonce,
            server_nonce,
            combined_nonce,
            salt: creds.salt,
            iteration_count: creds.iteration_count,
            auth_message: format!("{},{}", client_first.bare, server_first),
            stored_key,
            server_key,
            mechanism,
        };

        self.states.write().await.insert(session_id.clone(), state);

        Ok((server_first, session_id))
    }

    /// Handle client-final-message, return server-final-message and auth result
    pub async fn handle_client_final(
        &self,
        message: &str,
        session_id: &str,
    ) -> Result<(String, bool)> {
        // Get state
        let state = {
            let mut states = self.states.write().await;
            states.remove(session_id).ok_or(ScramError::InvalidState)?
        };

        // Parse client message
        let client_final = ClientFinalMessage::parse(message)?;

        // Verify nonce
        if client_final.nonce != state.combined_nonce {
            return Err(ScramError::NonceMismatch);
        }

        // Complete auth message
        let auth_message = format!("{},{}", state.auth_message, client_final.without_proof);

        // Compute expected client signature and proof
        let (client_signature, server_signature) = match state.mechanism {
            ScramMechanism::ScramSha256 => {
                self.compute_signatures_sha256(&state.stored_key, &state.server_key, &auth_message)
            }
            ScramMechanism::ScramSha512 => {
                self.compute_signatures_sha512(&state.stored_key, &state.server_key, &auth_message)
            }
        };

        // Decode client proof
        let client_proof = STANDARD
            .decode(&client_final.proof)
            .map_err(|e| ScramError::InvalidFormat(e.to_string()))?;

        // ClientProof = ClientKey XOR ClientSignature
        // ClientKey = ClientProof XOR ClientSignature
        // StoredKey = H(ClientKey)
        // Verify: H(ClientProof XOR ClientSignature) == StoredKey
        let recovered_client_key: Vec<u8> = client_proof
            .iter()
            .zip(client_signature.iter())
            .map(|(p, s)| p ^ s)
            .collect();

        let recovered_stored_key = match state.mechanism {
            ScramMechanism::ScramSha256 => Sha256::digest(&recovered_client_key).to_vec(),
            ScramMechanism::ScramSha512 => Sha512::digest(&recovered_client_key).to_vec(),
        };

        let authenticated = recovered_stored_key == state.stored_key;

        // Create server-final-message
        let server_final = if authenticated {
            format!("v={}", STANDARD.encode(&server_signature))
        } else {
            "e=invalid-proof".to_string()
        };

        Ok((server_final, authenticated))
    }

    fn compute_signatures_sha256(
        &self,
        stored_key: &[u8],
        server_key: &[u8],
        auth_message: &str,
    ) -> (Vec<u8>, Vec<u8>) {
        // ClientSignature = HMAC(StoredKey, AuthMessage)
        let mut client_sig_hmac = Hmac::<Sha256>::new_from_slice(stored_key).unwrap();
        client_sig_hmac.update(auth_message.as_bytes());
        let client_signature = client_sig_hmac.finalize().into_bytes().to_vec();

        // ServerSignature = HMAC(ServerKey, AuthMessage)
        let mut server_sig_hmac = Hmac::<Sha256>::new_from_slice(server_key).unwrap();
        server_sig_hmac.update(auth_message.as_bytes());
        let server_signature = server_sig_hmac.finalize().into_bytes().to_vec();

        (client_signature, server_signature)
    }

    fn compute_signatures_sha512(
        &self,
        stored_key: &[u8],
        server_key: &[u8],
        auth_message: &str,
    ) -> (Vec<u8>, Vec<u8>) {
        // ClientSignature = HMAC(StoredKey, AuthMessage)
        let mut client_sig_hmac = Hmac::<Sha512>::new_from_slice(stored_key).unwrap();
        client_sig_hmac.update(auth_message.as_bytes());
        let client_signature = client_sig_hmac.finalize().into_bytes().to_vec();

        // ServerSignature = HMAC(ServerKey, AuthMessage)
        let mut server_sig_hmac = Hmac::<Sha512>::new_from_slice(server_key).unwrap();
        server_sig_hmac.update(auth_message.as_bytes());
        let server_signature = server_sig_hmac.finalize().into_bytes().to_vec();

        (client_signature, server_signature)
    }

    /// Get supported mechanisms
    pub fn supported_mechanisms(&self) -> &[ScramMechanism] {
        &self.config.mechanisms
    }

    /// Clean up expired states
    pub async fn cleanup(&self) {
        // @Note, states would have timestamps
        // and be cleaned up based on age
    }
}

// ============================================================================
// Client-side SCRAM implementation (for testing and SDK use)
// ============================================================================

/// SCRAM client for authentication
pub struct ScramClient {
    username: String,
    password: String,
    mechanism: ScramMechanism,
    client_nonce: String,
    server_first: Option<String>,
    auth_message: Option<String>,
    salted_password: Option<Vec<u8>>,
}

impl ScramClient {
    /// Create a new SCRAM client
    pub fn new(username: &str, password: &str, mechanism: ScramMechanism) -> Self {
        // Generate client nonce
        let mut nonce_bytes = [0u8; 24];
        getrandom::getrandom(&mut nonce_bytes).expect("Failed to generate random bytes");
        let client_nonce = STANDARD.encode(nonce_bytes);

        Self {
            username: username.to_string(),
            password: password.to_string(),
            mechanism,
            client_nonce,
            server_first: None,
            auth_message: None,
            salted_password: None,
        }
    }

    /// Generate client-first-message
    pub fn client_first(&self) -> String {
        format!("n,,n={},r={}", self.username, self.client_nonce)
    }

    /// Get the bare client-first (without GS2 header)
    fn client_first_bare(&self) -> String {
        format!("n={},r={}", self.username, self.client_nonce)
    }

    /// Process server-first-message, return client-final-message
    pub fn handle_server_first(&mut self, server_first: &str) -> Result<String> {
        // Parse server-first
        let mut nonce = None;
        let mut salt = None;
        let mut iterations = None;

        for attr in server_first.split(',') {
            if let Some((key, value)) = attr.split_once('=') {
                match key {
                    "r" => nonce = Some(value.to_string()),
                    "s" => salt = Some(value.to_string()),
                    "i" => iterations = value.parse::<u32>().ok(),
                    _ => {}
                }
            }
        }

        let nonce = nonce.ok_or_else(|| {
            ScramError::InvalidFormat("Missing nonce in server-first".to_string())
        })?;
        let salt = salt
            .ok_or_else(|| ScramError::InvalidFormat("Missing salt in server-first".to_string()))?;
        let iterations = iterations.ok_or_else(|| {
            ScramError::InvalidFormat("Missing iterations in server-first".to_string())
        })?;

        // Verify nonce starts with our nonce
        if !nonce.starts_with(&self.client_nonce) {
            return Err(ScramError::NonceMismatch);
        }

        // Decode salt
        let salt_bytes = STANDARD
            .decode(&salt)
            .map_err(|e| ScramError::InvalidFormat(e.to_string()))?;

        // Derive salted password
        let salted_password = match self.mechanism {
            ScramMechanism::ScramSha256 => {
                let mut sp = vec![0u8; 32];
                pbkdf2_hmac::<Sha256>(self.password.as_bytes(), &salt_bytes, iterations, &mut sp);
                sp
            }
            ScramMechanism::ScramSha512 => {
                let mut sp = vec![0u8; 64];
                pbkdf2_hmac::<Sha512>(self.password.as_bytes(), &salt_bytes, iterations, &mut sp);
                sp
            }
        };

        // Build client-final-without-proof
        let gs2_header = "n,,";
        let channel_binding = STANDARD.encode(gs2_header);
        let client_final_without_proof = format!("c={},r={}", channel_binding, nonce);

        // Build auth message
        let auth_message = format!(
            "{},{},{}",
            self.client_first_bare(),
            server_first,
            client_final_without_proof
        );

        // Compute client proof
        let proof = match self.mechanism {
            ScramMechanism::ScramSha256 => {
                self.compute_proof_sha256(&salted_password, &auth_message)
            }
            ScramMechanism::ScramSha512 => {
                self.compute_proof_sha512(&salted_password, &auth_message)
            }
        };

        // Store for verification
        self.server_first = Some(server_first.to_string());
        self.auth_message = Some(auth_message);
        self.salted_password = Some(salted_password);

        Ok(format!(
            "{},p={}",
            client_final_without_proof,
            STANDARD.encode(&proof)
        ))
    }

    fn compute_proof_sha256(&self, salted_password: &[u8], auth_message: &str) -> Vec<u8> {
        // ClientKey = HMAC(SaltedPassword, "Client Key")
        let mut client_key_hmac = Hmac::<Sha256>::new_from_slice(salted_password).unwrap();
        client_key_hmac.update(b"Client Key");
        let client_key = client_key_hmac.finalize().into_bytes();

        // StoredKey = H(ClientKey)
        let stored_key = Sha256::digest(client_key);

        // ClientSignature = HMAC(StoredKey, AuthMessage)
        let mut client_sig_hmac = Hmac::<Sha256>::new_from_slice(&stored_key).unwrap();
        client_sig_hmac.update(auth_message.as_bytes());
        let client_signature = client_sig_hmac.finalize().into_bytes();

        // ClientProof = ClientKey XOR ClientSignature
        client_key
            .iter()
            .zip(client_signature.iter())
            .map(|(k, s)| k ^ s)
            .collect()
    }

    fn compute_proof_sha512(&self, salted_password: &[u8], auth_message: &str) -> Vec<u8> {
        // ClientKey = HMAC(SaltedPassword, "Client Key")
        let mut client_key_hmac = Hmac::<Sha512>::new_from_slice(salted_password).unwrap();
        client_key_hmac.update(b"Client Key");
        let client_key = client_key_hmac.finalize().into_bytes();

        // StoredKey = H(ClientKey)
        let stored_key = Sha512::digest(client_key);

        // ClientSignature = HMAC(StoredKey, AuthMessage)
        let mut client_sig_hmac = Hmac::<Sha512>::new_from_slice(&stored_key).unwrap();
        client_sig_hmac.update(auth_message.as_bytes());
        let client_signature = client_sig_hmac.finalize().into_bytes();

        // ClientProof = ClientKey XOR ClientSignature
        client_key
            .iter()
            .zip(client_signature.iter())
            .map(|(k, s)| k ^ s)
            .collect()
    }

    /// Verify server-final-message
    pub fn verify_server_final(&self, server_final: &str) -> Result<bool> {
        // Check for error
        if let Some(error_msg) = server_final.strip_prefix("e=") {
            return Err(ScramError::AuthenticationFailed(error_msg.to_string()));
        }

        // Parse server signature
        let server_signature = if let Some(sig) = server_final.strip_prefix("v=") {
            sig
        } else {
            return Err(ScramError::InvalidFormat(
                "Invalid server-final format".to_string(),
            ));
        };

        let salted_password = self
            .salted_password
            .as_ref()
            .ok_or(ScramError::InvalidState)?;

        let auth_message = self.auth_message.as_ref().ok_or(ScramError::InvalidState)?;

        // Compute expected server signature
        let expected = match self.mechanism {
            ScramMechanism::ScramSha256 => {
                let mut server_key_hmac = Hmac::<Sha256>::new_from_slice(salted_password).unwrap();
                server_key_hmac.update(b"Server Key");
                let server_key = server_key_hmac.finalize().into_bytes();

                let mut server_sig_hmac = Hmac::<Sha256>::new_from_slice(&server_key).unwrap();
                server_sig_hmac.update(auth_message.as_bytes());
                STANDARD.encode(server_sig_hmac.finalize().into_bytes())
            }
            ScramMechanism::ScramSha512 => {
                let mut server_key_hmac = Hmac::<Sha512>::new_from_slice(salted_password).unwrap();
                server_key_hmac.update(b"Server Key");
                let server_key = server_key_hmac.finalize().into_bytes();

                let mut server_sig_hmac = Hmac::<Sha512>::new_from_slice(&server_key).unwrap();
                server_sig_hmac.update(auth_message.as_bytes());
                STANDARD.encode(server_sig_hmac.finalize().into_bytes())
            }
        };

        if server_signature != expected {
            return Err(ScramError::ServerSignatureMismatch);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mechanism_parsing() {
        assert!(matches!(
            ScramMechanism::from_str("SCRAM-SHA-256"),
            Ok(ScramMechanism::ScramSha256)
        ));
        assert!(matches!(
            ScramMechanism::from_str("scram-sha-512"),
            Ok(ScramMechanism::ScramSha512)
        ));
        assert!(ScramMechanism::from_str("invalid").is_err());
    }

    #[test]
    fn test_client_first_parsing() {
        let msg = "n,,n=user,r=fyko+d2lbbFgONRv9qkxdawL";
        let parsed = ClientFirstMessage::parse(msg).unwrap();

        assert_eq!(parsed.username, "user");
        assert_eq!(parsed.nonce, "fyko+d2lbbFgONRv9qkxdawL");
        assert_eq!(parsed.gs2_header, "n,,");
    }

    #[test]
    fn test_client_final_parsing() {
        let msg =
            "c=biws,r=fyko+d2lbbFgONRv9qkxdawL3rfcNHYJY1ZVvWVs7j,p=v0X8v3Bz2T0CJGbJQyF0X+HI4Ts=";
        let parsed = ClientFinalMessage::parse(msg).unwrap();

        assert_eq!(parsed.channel_binding, "biws");
        assert!(parsed.nonce.starts_with("fyko+d2lbbFgONRv9qkxdawL"));
        assert!(!parsed.proof.is_empty());
    }

    #[test]
    fn test_config_defaults() {
        let config = ScramConfig::default();
        assert!(config.supports(ScramMechanism::ScramSha256));
        assert!(config.supports(ScramMechanism::ScramSha512));
        assert_eq!(config.iteration_count, 4096);
    }

    #[tokio::test]
    async fn test_full_authentication_sha256() {
        let server = ScramServer::new(ScramConfig::default());

        // Store credentials
        server
            .store_credentials("testuser", "testpassword", ScramMechanism::ScramSha256)
            .await
            .unwrap();

        // Create client
        let mut client = ScramClient::new("testuser", "testpassword", ScramMechanism::ScramSha256);

        // Step 1: Client sends client-first
        let client_first = client.client_first();

        // Step 2: Server responds with server-first
        let (server_first, session_id) = server
            .handle_client_first(&client_first, ScramMechanism::ScramSha256)
            .await
            .unwrap();

        // Step 3: Client sends client-final
        let client_final = client.handle_server_first(&server_first).unwrap();

        // Step 4: Server responds with server-final
        let (server_final, authenticated) = server
            .handle_client_final(&client_final, &session_id)
            .await
            .unwrap();

        assert!(authenticated);

        // Step 5: Client verifies server signature
        let verified = client.verify_server_final(&server_final).unwrap();
        assert!(verified);
    }

    #[tokio::test]
    async fn test_full_authentication_sha512() {
        let server = ScramServer::new(ScramConfig::default());

        // Store credentials
        server
            .store_credentials("testuser", "testpassword", ScramMechanism::ScramSha512)
            .await
            .unwrap();

        // Create client
        let mut client = ScramClient::new("testuser", "testpassword", ScramMechanism::ScramSha512);

        // Full authentication exchange
        let client_first = client.client_first();
        let (server_first, session_id) = server
            .handle_client_first(&client_first, ScramMechanism::ScramSha512)
            .await
            .unwrap();

        let client_final = client.handle_server_first(&server_first).unwrap();
        let (server_final, authenticated) = server
            .handle_client_final(&client_final, &session_id)
            .await
            .unwrap();

        assert!(authenticated);

        let verified = client.verify_server_final(&server_final).unwrap();
        assert!(verified);
    }

    #[tokio::test]
    async fn test_wrong_password() {
        let server = ScramServer::new(ScramConfig::default());

        server
            .store_credentials("testuser", "correctpassword", ScramMechanism::ScramSha256)
            .await
            .unwrap();

        // Client with wrong password
        let mut client = ScramClient::new("testuser", "wrongpassword", ScramMechanism::ScramSha256);

        let client_first = client.client_first();
        let (server_first, session_id) = server
            .handle_client_first(&client_first, ScramMechanism::ScramSha256)
            .await
            .unwrap();

        let client_final = client.handle_server_first(&server_first).unwrap();
        let (server_final, authenticated) = server
            .handle_client_final(&client_final, &session_id)
            .await
            .unwrap();

        // Should not authenticate
        assert!(!authenticated);
        assert!(server_final.starts_with("e="));
    }

    #[tokio::test]
    async fn test_user_not_found() {
        let server = ScramServer::new(ScramConfig::default());

        let client = ScramClient::new("nonexistent", "password", ScramMechanism::ScramSha256);
        let client_first = client.client_first();

        let result = server
            .handle_client_first(&client_first, ScramMechanism::ScramSha256)
            .await;

        assert!(matches!(result, Err(ScramError::UserNotFound(_))));
    }
}
