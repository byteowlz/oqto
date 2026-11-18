//! Extracted from Mobile9f7995c personal/transport.rs. Framing is exclusively
//! the shared xlatch-wire-auth implementation re-exported by Personal wire.
use crate::{ClientError, Result, client::now};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use oqto_personal_wire::{Enrollment, OQTO_PERSONAL_V1, SignedRequest, actions::HostAction};
use serde::{Deserialize, Serialize};

// Intentionally neither public nor Debug. Only opaque credential-store values
// cross the platform seam; public host DTOs never contain key material.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credential {
    seed: String,
    pub device_id: Option<String>,
}
impl Credential {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|_| ClientError::Storage)?;
        Ok(Self {
            seed: STANDARD.encode(bytes),
            device_id: None,
        })
    }
    fn key(&self) -> Result<SigningKey> {
        let bytes: [u8; 32] = STANDARD
            .decode(&self.seed)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or(ClientError::Storage)?;
        Ok(SigningKey::from_bytes(&bytes))
    }
    pub fn validate(&self) -> Result<()> {
        self.key()?;
        if !self
            .device_id
            .as_deref()
            .is_some_and(crate::client::uuid_id)
        {
            return Err(ClientError::Denied);
        }
        Ok(())
    }
    pub fn enrollment(&self, token: &str, name: &str) -> Result<Enrollment> {
        let key = self.key()?;
        let mut proof = Enrollment {
            token: token.into(),
            name: name.into(),
            public_key: STANDARD.encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        };
        proof.signature = STANDARD.encode(
            key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        Ok(proof)
    }
    pub fn request(&self, action: &HostAction) -> Result<SignedRequest> {
        self.validate()?;
        let key = self.key()?;
        let mut proof = SignedRequest {
            device_id: self.device_id.clone().ok_or(ClientError::Denied)?,
            timestamp: now()?,
            nonce: uuid::Uuid::new_v4().to_string(),
            payload: serde_json::to_string(action).map_err(|_| ClientError::InvalidInput)?,
            signature: String::new(),
        };
        proof.signature = STANDARD.encode(
            key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        Ok(proof)
    }
}
