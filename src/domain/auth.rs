use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub access_expires_in: u64,  // seconds
    pub refresh_token: String,
    pub refresh_expires_in: u64,  // seconds
}
