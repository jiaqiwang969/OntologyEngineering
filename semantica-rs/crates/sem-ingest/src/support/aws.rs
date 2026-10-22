//! HMAC-SHA256 and AWS Signature Version 4, used by the S3 and Kinesis REST clients (and the
//! Azure Shared Key signer).

use sha2::{Digest, Sha256};

/// `hmac.new(key, msg, hashlib.sha256).digest()`.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut k = if key.len() > BLOCK {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    k.resize(BLOCK, 0);
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner);
    outer.finalize().to_vec()
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// AWS credentials.
#[derive(Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    /// Access key id.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
    /// Session token (temporary credentials).
    pub session_token: Option<String>,
}

impl std::fmt::Debug for AwsCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsCredentials")
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"***")
            .finish()
    }
}

impl AwsCredentials {
    /// Explicit keys, else `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN`.
    pub fn resolve(access_key_id: Option<&str>, secret_access_key: Option<&str>) -> Option<AwsCredentials> {
        let key = access_key_id
            .map(str::to_string)
            .or_else(|| std::env::var("AWS_ACCESS_KEY_ID").ok())?;
        let secret = secret_access_key
            .map(str::to_string)
            .or_else(|| std::env::var("AWS_SECRET_ACCESS_KEY").ok())?;
        Some(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
        })
    }
}

/// RFC 3986 encoding as SigV4 requires (`/` kept when `keep_slash`).
pub fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') || (keep_slash && c == '/') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Signs a request with SigV4 and returns the headers to send (`Authorization`,
/// `x-amz-date`, `x-amz-content-sha256`, `host`, session token when present).
#[allow(clippy::too_many_arguments)]
pub fn sign_v4(
    credentials: &AwsCredentials,
    region: &str,
    service: &str,
    method: &str,
    host: &str,
    canonical_uri: &str,
    query: &[(String, String)],
    extra_headers: &[(String, String)],
    payload: &[u8],
    amz_date: &str,
) -> Vec<(String, String)> {
    let date = &amz_date[..8];
    let payload_hash = sha256_hex(payload);
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), host.to_string()),
        ("x-amz-content-sha256".to_string(), payload_hash.clone()),
        ("x-amz-date".to_string(), amz_date.to_string()),
    ];
    if let Some(token) = &credentials.session_token {
        headers.push(("x-amz-security-token".to_string(), token.clone()));
    }
    for (k, v) in extra_headers {
        headers.push((k.to_lowercase(), v.trim().to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));
    let mut q: Vec<(String, String)> = query
        .iter()
        .map(|(k, v)| (uri_encode(k, false), uri_encode(v, false)))
        .collect();
    q.sort();
    let canonical_query = q
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(format!("AWS4{}", credentials.secret_access_key).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );
    let mut out: Vec<(String, String)> = headers.into_iter().filter(|(k, _)| k != "host").collect();
    out.push(("Authorization".to_string(), authorization));
    out
}

/// The current time as `YYYYMMDDTHHMMSSZ`.
pub fn amz_now() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_vector() {
        assert_eq!(
            hex::encode(hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog")),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn sigv4_aws_example() {
        // The "GET object" example of the S3 SigV4 documentation.
        let creds = AwsCredentials {
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: None,
        };
        let headers = sign_v4(
            &creds,
            "us-east-1",
            "s3",
            "GET",
            "examplebucket.s3.amazonaws.com",
            "/test.txt",
            &[],
            &[("range".to_string(), "bytes=0-9".to_string())],
            b"",
            "20130524T000000Z",
        );
        let auth = headers.iter().find(|(k, _)| k == "Authorization").unwrap();
        assert!(auth.1.ends_with(
            "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        ));
    }
}
