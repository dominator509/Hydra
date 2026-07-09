use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use time::{Duration, OffsetDateTime};

type HmacSha256 = Hmac<Sha256>;

/// Pre-defined token scopes used for capability-based access control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenScope {
    /// Read the CDM entity graph.
    ReadCdm,
    /// Write envelopes (propose, update).
    WriteEnvelopes,
    /// Approve/reject envelopes.
    ApproveEnvelopes,
    /// Administer bridges (register, pause, resume).
    AdminBridges,
    /// Administer autonomy cells.
    AdminAutonomy,
}

impl TokenScope {
    /// Parse a space-separated scope string into a `Vec<TokenScope>`.
    ///
    /// Unknown scope names are silently ignored.
    pub fn parse_all(input: &str) -> Vec<TokenScope> {
        input.split_whitespace().filter_map(|s| match s {
            "read:cdm" => Some(TokenScope::ReadCdm),
            "write:envelopes" => Some(TokenScope::WriteEnvelopes),
            "approve:envelopes" => Some(TokenScope::ApproveEnvelopes),
            "admin:bridges" => Some(TokenScope::AdminBridges),
            "admin:autonomy" => Some(TokenScope::AdminAutonomy),
            _ => None,
        }).collect()
    }

    /// Canonical string representation of this scope (the claim value).
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenScope::ReadCdm => "read:cdm",
            TokenScope::WriteEnvelopes => "write:envelopes",
            TokenScope::ApproveEnvelopes => "approve:envelopes",
            TokenScope::AdminBridges => "admin:bridges",
            TokenScope::AdminAutonomy => "admin:autonomy",
        }
    }
}

/// JWT claims following the standard RFC 7519 layout plus a custom `scope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject — who the token identifies.
    pub sub: String,
    /// Audience — the tenant UUID.
    pub aud: uuid::Uuid,
    /// Issuer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Expiration (Unix timestamp).
    pub exp: i64,
    /// Issued-at (Unix timestamp).
    pub iat: i64,
    /// Space-separated scope string.
    pub scope: String,
}

impl TokenClaims {
    /// Build a new set of claims valid for `ttl_hours` from now.
    pub fn new(subject: String, audience: uuid::Uuid, scopes: &[TokenScope], ttl_hours: i64) -> Self {
        let now = OffsetDateTime::now_utc();
        let scope = scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            sub: subject,
            aud: audience,
            iss: Some("hydra".into()),
            exp: (now + Duration::hours(ttl_hours)).unix_timestamp(),
            iat: now.unix_timestamp(),
            scope,
        }
    }
}

/// A simple HMAC-SHA256 JWT implementation for development use.
///
/// **Do not use with the production secret in test environments.**
#[derive(Clone)]
pub struct TokenService {
    secret: Vec<u8>,
}

impl TokenService {
    /// Create a new service with the given signing secret.
    pub fn new(secret: Vec<u8>) -> Self {
        Self { secret }
    }

    /// Sign claims into a compact JWT string.
    pub fn sign(&self, claims: &TokenClaims) -> Result<String, String> {
        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let header_b64 = encode_b64url(&serde_json::to_vec(&header).map_err(|e| e.to_string())?);
        let payload_b64 =
            encode_b64url(&serde_json::to_vec(claims).map_err(|e| e.to_string())?);

        let signing_input = format!("{header_b64}.{payload_b64}");
        let signature = self.sign_raw(signing_input.as_bytes())?;
        let signature_b64 = encode_b64url(&signature);

        Ok(format!("{signing_input}.{signature_b64}"))
    }

    /// Verify a compact JWT string and return the parsed claims.
    pub fn verify(&self, token: &str) -> Result<TokenClaims, String> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err("token must have exactly 3 dot-separated segments".into());
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let actual_sig =
            decode_b64url(parts[2]).map_err(|e| format!("invalid signature encoding: {e}"))?;

        // Compute HMAC and compare in constant time
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|e| format!("HMAC init: {e}"))?;
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&actual_sig)
            .map_err(|_| "signature mismatch".to_string())?;

        let payload_bytes =
            decode_b64url(parts[1]).map_err(|e| format!("invalid payload encoding: {e}"))?;
        serde_json::from_slice(&payload_bytes).map_err(|e| format!("invalid claims JSON: {e}"))
    }

    fn sign_raw(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|e| format!("HMAC key init: {e}"))?;
        mac.update(data);
        Ok(mac.finalize().into_bytes().to_vec())
    }
}

// ---------------------------------------------------------------------------
// Base64url encoding / decoding (no external crate needed)
// ---------------------------------------------------------------------------

static B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_b64url(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

fn decode_b64url(input: &str) -> Result<Vec<u8>, String> {
    // Build reverse lookup
    let mut rev = [0xffu8; 256];
    for (i, &c) in B64_CHARS.iter().enumerate() {
        rev[c as usize] = i as u8;
    }

    let bytes: Vec<u8> = input.bytes().collect();
    // Padding is optional in base64url; calculate actual byte count
    let mut out = Vec::with_capacity((bytes.len() * 3).div_ceil(4));
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            return Err("base64url input chunk too short".into());
        }
        let mut quad = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if rev[b as usize] == 0xff {
                return Err(format!("invalid base64url character: {}", b as char));
            }
            quad[i] = rev[b as usize];
        }

        let triple = ((quad[0] as u32) << 18)
            | ((quad[1] as u32) << 12)
            | ((quad[2] as u32) << 6)
            | (quad[3] as u32);

        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn base64url_roundtrip() {
        let data = b"hello world";
        let encoded = encode_b64url(data);
        let decoded = decode_b64url(&encoded).expect("base64 roundtrip");
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64url_padding_agnostic() {
        // base64url "aGVsbG8" (no padding) should decode to "hello"
        let decoded = decode_b64url("aGVsbG8").expect("base64 roundtrip");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let secret = b"test-secret".to_vec();
        let svc = TokenService::new(secret);
        let claims = TokenClaims::new(
            "service:my-app".into(),
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("base64 roundtrip"),
            &[TokenScope::ReadCdm, TokenScope::WriteEnvelopes],
            1,
        );

        let token = svc.sign(&claims).expect("base64 roundtrip");
        let verified = svc.verify(&token).expect("base64 roundtrip");

        assert_eq!(verified.sub, claims.sub);
        assert_eq!(verified.aud, claims.aud);
        assert_eq!(verified.scope, claims.scope);
    }

    #[test]
    fn verify_rejects_tampered_token() {
        let secret = b"test-secret".to_vec();
        let svc = TokenService::new(secret);
        let claims = TokenClaims::new(
            "service:test".into(),
            Uuid::nil(),
            &[TokenScope::ReadCdm],
            1,
        );

        let token = svc.sign(&claims).expect("base64 roundtrip");
        // Mutate a character in the payload segment
        let mut parts: Vec<&str> = token.split('.').collect();
        let mut payload_b64 = parts[1].to_string();
        payload_b64.truncate(payload_b64.len() - 1); // corrupt
        parts[1] = &payload_b64;
        let tampered = parts.join(".");

        assert!(svc.verify(&tampered).is_err());
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let svc1 = TokenService::new(b"secret-1".to_vec());
        let svc2 = TokenService::new(b"secret-2".to_vec());
        let claims = TokenClaims::new(
            "sub".into(),
            Uuid::nil(),
            &[TokenScope::ReadCdm],
            1,
        );

        let token = svc1.sign(&claims).expect("base64 roundtrip");
        assert!(svc2.verify(&token).is_err());
    }

    #[test]
    fn parse_all_scope_string() {
        let scopes = TokenScope::parse_all("read:cdm write:envelopes admin:bridges unknown:thing");
        assert_eq!(scopes.len(), 3);
        assert!(scopes.contains(&TokenScope::ReadCdm));
        assert!(scopes.contains(&TokenScope::WriteEnvelopes));
        assert!(scopes.contains(&TokenScope::AdminBridges));
    }

    #[test]
    fn parse_all_empty() {
        let scopes = TokenScope::parse_all("");
        assert!(scopes.is_empty());
    }
}
