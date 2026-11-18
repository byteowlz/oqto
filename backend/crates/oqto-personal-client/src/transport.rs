//! One-attempt pinned HTTPS extracted from Mobile9f7995c transport. No CA,
//! redirect, proxy or retry fallback. No raw transport/body errors escape.
use crate::{ClientError, Host, Result, credential::Credential};
use oqto_personal_wire::{Enrollment, actions::HostAction};
use serde_json::Value;
use std::{collections::HashMap, error::Error, sync::Mutex, time::Duration};

const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
const HISTORY_LIMIT: usize = 12 * 1024 * 1024;
const POOL_HOSTS: usize = 32;

#[derive(Default)]
pub(crate) struct Transport {
    pooled: Mutex<HashMap<(String, String, String), reqwest::Client>>,
}
impl Transport {
    pub fn clear(&self) {
        if let Ok(mut pool) = self.pooled.lock() {
            pool.clear();
        }
    }
    pub async fn enroll(&self, host: &Host, proof: &Enrollment) -> Result<Value> {
        let body = serde_json::to_vec(proof).map_err(|_| ClientError::InvalidInput)?;
        if body.len() > oqto_personal_wire::MAX_ENROLLMENT_BYTES {
            return Err(ClientError::InvalidInput);
        }
        self.post(
            host,
            oqto_personal_wire::ENROLL_PATH,
            body,
            Exchange {
                uncertain: true,
                wait: false,
                pool: false,
                attach: false,
                limit: RESPONSE_LIMIT,
            },
        )
        .await
    }
    pub async fn action(
        &self,
        host: &Host,
        key: &Credential,
        action: &HostAction,
    ) -> Result<Value> {
        let proof = key.request(action)?;
        let body = serde_json::to_vec(&proof).map_err(|_| ClientError::InvalidInput)?;
        if body.len() > oqto_personal_wire::actions::MAX_ACTION_BYTES {
            return Err(ClientError::InvalidInput);
        }
        self.post(
            host,
            oqto_personal_wire::actions::ACTION_PATH,
            body,
            Exchange {
                uncertain: mutating(action),
                wait: matches!(action, HostAction::SessionRequestControl { .. }),
                pool: !mutating(action)
                    && !matches!(action, HostAction::SessionReleaseControl { .. }),
                attach: matches!(action, HostAction::SessionAttach { .. }),
                limit: if matches!(action, HostAction::SessionHistory { .. }) {
                    HISTORY_LIMIT
                } else {
                    RESPONSE_LIMIT
                },
            },
        )
        .await
    }
    fn client(&self, host: &Host, wait: bool, pool: bool) -> Result<reqwest::Client> {
        let name = (
            host.id.clone(),
            host.registration_id.clone(),
            host.certificate_sha256.clone(),
        );
        if pool
            && let Some(client) = self
                .pooled
                .lock()
                .map_err(|_| ClientError::Storage)?
                .get(&name)
        {
            return Ok(client.clone());
        }
        let tls = oqto_personal_wire::tls::pinned_client_config(&host.certificate_sha256)
            .map_err(|_| ClientError::InvalidInput)?;
        let builder = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .https_only(true)
            .http1_only()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(if wait { 40 } else { 10 }));
        let builder = if pool {
            builder
                .pool_max_idle_per_host(4)
                .pool_idle_timeout(Duration::from_secs(60))
                .tcp_keepalive(Duration::from_secs(30))
        } else {
            builder.pool_max_idle_per_host(0)
        };
        let client = builder.build().map_err(|_| ClientError::Offline)?;
        if pool {
            let mut clients = self.pooled.lock().map_err(|_| ClientError::Storage)?;
            if clients.len() >= POOL_HOSTS {
                clients.clear();
            }
            clients.insert(name, client.clone());
        }
        Ok(client)
    }
    async fn post(
        &self,
        host: &Host,
        path: &str,
        bytes: Vec<u8>,
        exchange: Exchange,
    ) -> Result<Value> {
        let client = self.client(host, exchange.wait, exchange.pool && !exchange.uncertain)?;
        let mut request = client
            .post(format!("{}{path}", host.origin.trim_end_matches('/')))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes);
        if exchange.limit > RESPONSE_LIMIT {
            request = request.timeout(Duration::from_secs(30));
        }
        let mut response = request.send().await.map_err(|error| {
            if pin_mismatch(&error) {
                ClientError::PinMismatch
            } else {
                exchange.failed()
            }
        })?;
        if !response.status().is_success() {
            return Err(match response.status().as_u16() {
                401 | 403 => ClientError::Denied,
                409 => ClientError::OutcomeUnknown,
                503 if !exchange.uncertain || exchange.attach => ClientError::NotReady,
                _ => exchange.failed(),
            });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| exchange.failed())? {
            if body.len().saturating_add(chunk.len()) > exchange.limit {
                return Err(if exchange.uncertain {
                    ClientError::OutcomeUnknown
                } else {
                    ClientError::InvalidResponse
                });
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_| {
            if exchange.uncertain {
                ClientError::OutcomeUnknown
            } else {
                ClientError::InvalidResponse
            }
        })
    }
}

struct Exchange {
    uncertain: bool,
    wait: bool,
    pool: bool,
    attach: bool,
    limit: usize,
}
impl Exchange {
    fn failed(&self) -> ClientError {
        if self.uncertain {
            ClientError::OutcomeUnknown
        } else {
            ClientError::Offline
        }
    }
}

fn pin_mismatch(error: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(current) = source {
        // Inspect the typed error emitted by the pinned wire verifier, never
        // arbitrary response text or a formatted error containing private data.
        if matches!(current.downcast_ref::<tokio_rustls::rustls::Error>(), Some(tokio_rustls::rustls::Error::General(message)) if message == "paired host certificate pin mismatch")
        {
            return true;
        }
        // Hyper may wrap the TLS error in multiple io::Error values. source()
        // skips their payload; get_ref() preserves the actual typed verifier error.
        source = current
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .map(|error| error as &(dyn Error + 'static))
            .or_else(|| current.source());
    }
    false
}

pub(crate) fn mutating(action: &HostAction) -> bool {
    !matches!(
        action,
        HostAction::Capabilities
            | HostAction::SessionsList
            | HostAction::ProjectIcons { .. }
            | HostAction::SessionFiles { .. }
            | HostAction::SessionHistory { .. }
            | HostAction::SessionMedia { .. }
            | HostAction::SessionRead { .. }
            | HostAction::SessionModels { .. }
            | HostAction::FilesList { .. }
            | HostAction::FileRead { .. }
            | HostAction::TerminalsList
            | HostAction::TerminalRead { .. }
    )
}
