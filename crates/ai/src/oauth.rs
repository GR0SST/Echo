//! Public OpenAI OSS registration flow, not another application's client ID.
//! https://developers.openai.com/siwc/token-sharing-open-source/sign-in
use crate::{
    client::{agent, status, AiError, Cancellation},
    credentials::{CredentialStore, Secret},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;
use zeroize::Zeroize;

const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
// All normal windows share this process. Serialize rotating-token refreshes and
// reload under the lock, so concurrent feature requests cannot reuse old tokens.
static REFRESH: Mutex<()> = Mutex::new(());

struct SessionLock(std::fs::File);
impl SessionLock {
    fn acquire(id: &str, cancel: &Cancellation) -> Result<Self, AiError> {
        let dir = dirs::cache_dir()
            .ok_or(AiError::Credentials)?
            .join("Echo")
            .join("ai-session-locks");
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&dir)
                .map_err(|_| AiError::Credentials)?;
            if std::fs::symlink_metadata(&dir)
                .map_err(|_| AiError::Credentials)?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err(AiError::Credentials);
            }
        }
        #[cfg(not(unix))]
        std::fs::create_dir_all(&dir).map_err(|_| AiError::Credentials)?;
        if std::fs::symlink_metadata(&dir)
            .map_err(|_| AiError::Credentials)?
            .file_type()
            .is_symlink()
        {
            return Err(AiError::Credentials);
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(URL_SAFE_NO_PAD.encode(Sha256::digest(id.as_bytes()))))
            .map_err(|_| AiError::Credentials)?;
        let deadline = Instant::now() + Duration::from_secs(130);
        loop {
            cancel.check()?;
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(Self(file)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(AiError::Credentials);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return Err(AiError::Credentials),
            }
        }
    }
}
impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

fn random() -> Result<String, AiError> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| AiError::Identity)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
pub fn new_credential_id() -> Result<String, AiError> {
    Ok(format!("max-{}", random()?))
}
pub fn new_host_id() -> Result<String, AiError> {
    let mut bytes = [0u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| AiError::Identity)?;
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let h: String = bytes.iter().map(|n| format!("{n:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |v, (a, b)| v | a ^ b) == 0
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub credential_id: String,
    pub subject: String,
    pub email: String,
    pub client_id: String,
    pub plan_enabled: bool,
}
#[derive(Serialize, Deserialize)]
struct Session {
    account: Account,
    access_token: String,
    refresh_token: String,
    id_token: String,
    scopes: String,
    expires_at: u64,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.id_token.zeroize();
    }
}
fn load(store: &dyn CredentialStore, id: &str) -> Result<Session, AiError> {
    let value = store
        .get(id)
        .map_err(|_| AiError::Credentials)?
        .ok_or(AiError::NotConfigured)?;
    let session: Session = serde_json::from_str(value.expose()).map_err(|_| AiError::Identity)?;
    if session.account.credential_id != id {
        return Err(AiError::Identity);
    }
    Ok(session)
}
fn save(store: &dyn CredentialStore, session: &Session) -> Result<(), AiError> {
    let value = Secret::new(serde_json::to_string(session).map_err(|_| AiError::Identity)?);
    store
        .set(&session.account.credential_id, &value)
        .map_err(|_| AiError::Credentials)
}
pub fn account(store: &dyn CredentialStore, id: &str) -> Result<Account, AiError> {
    Ok(load(store, id)?.account.clone())
}

pub struct PendingSignIn {
    listener: TcpListener,
    url: Secret,
    redirect: String,
    state: Secret,
    nonce: Secret,
    verifier: Secret,
    prior: Option<Account>,
    expected_client: Option<String>,
    pending_key: String,
}
impl PendingSignIn {
    /// Bind first, then open this URL in the system browser. Do not log it:
    /// returning authorization URLs can include the retained ID-token hint.
    pub fn start(
        store: &dyn CredentialStore,
        host_id: &str,
        prior_id: Option<&str>,
    ) -> Result<Self, AiError> {
        if !host_id.starts_with("urn:uuid:")
            && !host_id.starts_with("urn:ietf:params:oauth:jwk-thumbprint:")
        {
            return Err(AiError::Identity);
        }
        let prior = prior_id.map(|id| load(store, id)).transpose()?;
        let pending_key = format!(
            "max-registration-{}",
            URL_SAFE_NO_PAD.encode(Sha256::digest(host_id.as_bytes()))
        );
        let saved_client = if prior.is_none() {
            store
                .get(&pending_key)
                .map_err(|_| AiError::Credentials)?
                .and_then(|v| serde_json::from_str::<String>(v.expose()).ok())
        } else {
            None
        };
        let expected_client = prior
            .as_ref()
            .map(|s| s.account.client_id.clone())
            .or(saved_client);
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| AiError::Network)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| AiError::Network)?;
        let redirect = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener.local_addr().map_err(|_| AiError::Network)?.port()
        );
        let state = Secret::new(random()?);
        let nonce = Secret::new(random()?);
        let verifier = Secret::new(random()?);
        let mut url = Url::parse(AUTHORIZE).unwrap();
        {
            let mut q = url.query_pairs_mut();
            q.append_pair(
                "client_id",
                expected_client.as_deref().unwrap_or(DYNAMIC_CLIENT),
            );
            if expected_client.is_none() {
                q.append_pair("agent_name_hint", "Echo");
            }
            q.append_pair("ext_agent_host_id", host_id)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect)
                .append_pair("scope", SCOPES)
                .append_pair("resource", RESOURCE)
                .append_pair("state", state.expose())
                .append_pair("nonce", nonce.expose())
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge(verifier.expose()));
            if let Some(session) = &prior {
                if !session.id_token.is_empty() {
                    q.append_pair("id_token_hint", &session.id_token);
                }
                if !session.account.email.is_empty() {
                    q.append_pair("login_hint", &session.account.email);
                }
            }
        }
        Ok(Self {
            listener,
            url: Secret::new(url.into()),
            redirect,
            state,
            nonce,
            verifier,
            prior: prior.map(|s| s.account.clone()),
            expected_client,
            pending_key,
        })
    }
    pub fn authorization_url(&self) -> &str {
        self.url.expose()
    }
    /// Consumes the attempt so neither its callback nor PKCE verifier is reused.
    pub fn finish(
        self,
        store: &dyn CredentialStore,
        cancel: &Cancellation,
    ) -> Result<Account, AiError> {
        let deadline = Instant::now() + Duration::from_secs(180);
        let callback = loop {
            cancel.check()?;
            if Instant::now() >= deadline {
                return Err(AiError::Cancelled);
            }
            match self.listener.accept() {
                Ok((mut stream, remote)) => {
                    if !remote.ip().is_loopback() {
                        continue;
                    }
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .map_err(|_| AiError::Network)?;
                    let mut bytes = Vec::new();
                    let mut block = [0u8; 1024];
                    loop {
                        let count = stream.read(&mut block).map_err(|_| AiError::Network)?;
                        if count == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&block[..count]);
                        if bytes.len() > 16 * 1024 {
                            return Err(AiError::Identity);
                        }
                        if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let first = std::str::from_utf8(&bytes)
                        .map_err(|_| AiError::Identity)?
                        .lines()
                        .next()
                        .ok_or(AiError::Identity)?;
                    let mut parts = first.split_whitespace();
                    if parts.next() != Some("GET") {
                        continue;
                    }
                    let target = parts.next().ok_or(AiError::Identity)?;
                    let base = Url::parse(&self.redirect).unwrap();
                    let callback = base.join(target).map_err(|_| AiError::Identity)?;
                    if callback.path() != "/auth/callback" || callback.origin() != base.origin() {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        continue;
                    }
                    let valid = self.callback(&callback);
                    let page = if valid.is_ok() {
                        "Return to Echo to finish connecting."
                    } else {
                        "Sign-in could not be verified. Return to Echo and try again."
                    };
                    let _ = write!(stream, "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", if valid.is_ok() { "200 OK" } else { "400 Bad Request" }, page.len(), page);
                    break valid?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(_) => return Err(AiError::Network),
            }
        };
        cancel.check()?;
        let (code, client_id) = callback;
        // Registration is retained before exchanging a short-lived code. An
        // expired code must not cause another dynamic client registration.
        if self.prior.is_none() {
            let registration =
                Secret::new(serde_json::to_string(&client_id).map_err(|_| AiError::Identity)?);
            store
                .set(&self.pending_key, &registration)
                .map_err(|_| AiError::Credentials)?;
        }
        let response = token_request(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", code.expose()),
            ("code_verifier", self.verifier.expose()),
            ("redirect_uri", &self.redirect),
            ("resource", RESOURCE),
        ])?;
        let claims = verify_identity(&response.id_token, &client_id, self.nonce.expose())?;
        if self
            .prior
            .as_ref()
            .is_some_and(|p| p.subject != claims.sub || p.client_id != client_id)
        {
            return Err(AiError::Identity);
        }
        cancel.check()?;
        let id = self
            .prior
            .as_ref()
            .map(|p| p.credential_id.clone())
            .unwrap_or(new_credential_id()?);
        let account = Account {
            credential_id: id,
            subject: claims.sub,
            email: claims.email.unwrap_or_default(),
            client_id,
            plan_enabled: response
                .scope
                .split_whitespace()
                .any(|s| s == "chatgpt.tokens.use.direct"),
        };
        let session = Session {
            account: account.clone(),
            access_token: response.access_token.clone(),
            refresh_token: response.refresh_token.clone(),
            id_token: response.id_token.clone(),
            scopes: response.scope.clone(),
            expires_at: now() + response.expires_in,
        };
        save(store, &session)?;
        if self.prior.is_none() {
            let _ = store.delete(&self.pending_key);
        }
        Ok(account)
    }
    fn callback(&self, url: &Url) -> Result<(Secret, String), AiError> {
        let mut pairs = std::collections::BTreeMap::new();
        for (k, v) in url.query_pairs() {
            if pairs.insert(k.to_string(), v.to_string()).is_some() {
                return Err(AiError::Identity);
            }
        }
        if !same(
            pairs.get("state").ok_or(AiError::Identity)?,
            self.state.expose(),
        ) {
            return Err(AiError::Identity);
        }
        if pairs.contains_key("error") {
            return Err(AiError::Unauthorized);
        }
        let client_id = match (&self.expected_client, pairs.get("client_id")) {
            (Some(expected), supplied) if supplied.is_none_or(|id| id == expected) => {
                expected.clone()
            }
            (None, Some(issued))
                if !issued.is_empty() && issued != DYNAMIC_CLIENT && issued.len() < 256 =>
            {
                issued.clone()
            }
            _ => return Err(AiError::Identity),
        };
        let code = pairs
            .remove("code")
            .filter(|s| !s.is_empty())
            .ok_or(AiError::Identity)?;
        Ok((Secret::new(code), client_id))
    }
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    scope: String,
    expires_in: u64,
    token_type: String,
}
impl Drop for TokenResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.id_token.zeroize();
    }
}
fn token_request(form: &[(&str, &str)]) -> Result<TokenResponse, AiError> {
    let mut response = agent()
        .post(TOKEN)
        .send_form(form.iter().copied())
        .map_err(|_| AiError::Network)?;
    status(response.status().as_u16())?;
    let token: TokenResponse = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_json()
        .map_err(|_| AiError::Identity)?;
    if !token.token_type.eq_ignore_ascii_case("Bearer")
        || token.access_token.is_empty()
        || token.expires_in == 0
    {
        return Err(AiError::Identity);
    }
    Ok(token)
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    nonce: String,
}
fn verify_with_jwks(
    token: &str,
    client: &str,
    nonce: &str,
    jwks: &JwkSet,
) -> Result<Claims, AiError> {
    let header = decode_header(token).map_err(|_| AiError::Identity)?;
    if header.alg != Algorithm::RS256 {
        return Err(AiError::Identity);
    }
    let key = jwks
        .find(header.kid.as_deref().ok_or(AiError::Identity)?)
        .ok_or(AiError::Identity)?;
    let key = DecodingKey::from_jwk(key).map_err(|_| AiError::Identity)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client]);
    validation.leeway = 30;
    let data = decode::<Claims>(token, &key, &validation).map_err(|_| AiError::Identity)?;
    if data.claims.sub.is_empty() || !same(&data.claims.nonce, nonce) {
        return Err(AiError::Identity);
    }
    Ok(data.claims)
}
fn metadata() -> Result<serde_json::Value, AiError> {
    let mut response = agent()
        .get("https://auth.openai.com/.well-known/openid-configuration")
        .call()
        .map_err(|_| AiError::Network)?;
    status(response.status().as_u16())?;
    let data: serde_json::Value = response
        .body_mut()
        .with_config()
        .limit(128 * 1024)
        .read_json()
        .map_err(|_| AiError::Identity)?;
    if data["issuer"] != ISSUER {
        return Err(AiError::Identity);
    }
    Ok(data)
}
fn metadata_endpoint(metadata: &serde_json::Value, field: &str) -> Result<Url, AiError> {
    let url = Url::parse(metadata[field].as_str().ok_or(AiError::Identity)?)
        .map_err(|_| AiError::Identity)?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AiError::Identity);
    }
    Ok(url)
}
fn verify_identity(token: &str, client: &str, nonce: &str) -> Result<Claims, AiError> {
    let url = metadata_endpoint(&metadata()?, "jwks_uri")?;
    let mut response = agent()
        .get(url.as_str())
        .call()
        .map_err(|_| AiError::Network)?;
    status(response.status().as_u16())?;
    let keys: JwkSet = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_json()
        .map_err(|_| AiError::Identity)?;
    verify_with_jwks(token, client, nonce, &keys)
}
pub fn access_token(
    store: &dyn CredentialStore,
    id: &str,
    cancel: &Cancellation,
) -> Result<Secret, AiError> {
    cancel.check()?;
    let _lock = REFRESH.lock().map_err(|_| AiError::Credentials)?;
    let _session_lock = SessionLock::acquire(id, cancel)?;
    cancel.check()?;
    let mut session = load(store, id)?;
    if !session.account.plan_enabled {
        return Err(AiError::Unauthorized);
    }
    if session.expires_at <= now() + 60 {
        if session.refresh_token.is_empty() {
            return Err(AiError::Unauthorized);
        }
        let response = token_request(&[
            ("grant_type", "refresh_token"),
            ("client_id", &session.account.client_id),
            ("refresh_token", &session.refresh_token),
            ("resource", RESOURCE),
        ])?;
        cancel.check()?;
        session.access_token = response.access_token.clone();
        session.expires_at = now() + response.expires_in;
        if !response.refresh_token.is_empty() {
            session.refresh_token = response.refresh_token.clone();
        }
        if !response.scope.is_empty() {
            session.scopes = response.scope.clone();
            session.account.plan_enabled = session
                .scopes
                .split_whitespace()
                .any(|s| s == "chatgpt.tokens.use.direct");
        }
        save(store, &session)?;
        if !session.account.plan_enabled {
            return Err(AiError::Unauthorized);
        }
    }
    cancel.check()?;
    Ok(Secret::new(session.access_token.clone()))
}
/// Clear tokens even if remote revocation is unavailable; preserve registration
/// so signing out does not register another client on the next sign-in.
/// The return value tells settings whether remote revocation was confirmed.
pub fn sign_out(store: &dyn CredentialStore, id: &str) -> Result<bool, AiError> {
    let _lock = REFRESH.lock().map_err(|_| AiError::Credentials)?;
    let _session_lock = SessionLock::acquire(id, &Cancellation::default())?;
    let mut session = load(store, id)?;
    let revoked = (|| {
        let endpoint = metadata_endpoint(&metadata()?, "revocation_endpoint")?;
        let response = agent()
            .post(endpoint.as_str())
            .send_form([
                ("token", session.refresh_token.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", session.account.client_id.as_str()),
            ])
            .map_err(|_| AiError::Network)?;
        status(response.status().as_u16())
    })()
    .is_ok();
    session.access_token.zeroize();
    session.access_token.clear();
    session.refresh_token.zeroize();
    session.refresh_token.clear();
    session.id_token.zeroize();
    session.id_token.clear();
    session.expires_at = 0;
    session.account.plan_enabled = false;
    save(store, &session)?;
    Ok(revoked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::testing::MemoryStore;
    #[test]
    fn public_registration_uses_echo_name_fresh_pkce_and_validates_callback_before_exchange() {
        let store = MemoryStore::default();
        let host = new_host_id().unwrap();
        let pending = PendingSignIn::start(&store, &host, None).unwrap();
        let url = Url::parse(pending.authorization_url()).unwrap();
        let p: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(p["client_id"], DYNAMIC_CLIENT);
        assert_eq!(p["agent_name_hint"], "Echo");
        assert_eq!(p["ext_agent_host_id"], host);
        assert_eq!(p["code_challenge"], challenge(pending.verifier.expose()));
        assert_eq!(p["resource"], RESOURCE);
        let mut callback = Url::parse(&pending.redirect).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("code", "test-code")
            .append_pair("state", "wrong")
            .append_pair("client_id", "oaiapp_echo_test");
        assert!(pending.callback(&callback).is_err());
        callback.set_query(None);
        callback
            .query_pairs_mut()
            .append_pair("code", "test-code")
            .append_pair("state", pending.state.expose())
            .append_pair("client_id", "oaiapp_echo_test");
        assert_eq!(pending.callback(&callback).unwrap().1, "oaiapp_echo_test");
        callback.query_pairs_mut().append_pair("code", "duplicate");
        assert!(pending.callback(&callback).is_err());
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn identity_verification_requires_a_signed_token_for_this_client_issuer_and_nonce() {
        use rsa::{
            pkcs8::{EncodePrivateKey, LineEnding},
            traits::PublicKeyParts,
        };
        let private = rsa::RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let public = rsa::RsaPublicKey::from(&private);
        let keys:JwkSet=serde_json::from_value(serde_json::json!({"keys":[{"kty":"RSA","kid":"echo-test","alg":"RS256","use":"sig","n":URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(public.e().to_bytes_be())}]})).unwrap();
        let pem = private.to_pkcs8_pem(LineEnding::LF).unwrap();
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
        let mut header = jsonwebtoken::Header::new(Algorithm::RS256);
        header.kid = Some("echo-test".into());
        let claims = serde_json::json!({"iss":ISSUER,"aud":"oaiapp_echo","sub":"verified-user","email":"test@example.com","nonce":"fresh-nonce","exp":now()+300});
        let token = jsonwebtoken::encode(&header, &claims, &key).unwrap();
        assert_eq!(
            verify_with_jwks(&token, "oaiapp_echo", "fresh-nonce", &keys)
                .unwrap()
                .sub,
            "verified-user"
        );
        assert!(verify_with_jwks(&token, "another-client", "fresh-nonce", &keys).is_err());
        assert!(verify_with_jwks(&token, "oaiapp_echo", "wrong-nonce", &keys).is_err());
        let mut wrong = claims.clone();
        wrong["iss"] = "https://other.example".into();
        let wrong = jsonwebtoken::encode(&header, &wrong, &key).unwrap();
        assert!(verify_with_jwks(&wrong, "oaiapp_echo", "fresh-nonce", &keys).is_err());
        let mut expired = claims;
        expired["exp"] = (now() - 3600).into();
        let expired = jsonwebtoken::encode(&header, &expired, &key).unwrap();
        assert!(verify_with_jwks(&expired, "oaiapp_echo", "fresh-nonce", &keys).is_err());
        let mut tampered = token.into_bytes();
        let n = tampered.len() - 5;
        tampered[n] = if tampered[n] == b'A' { b'B' } else { b'A' };
        assert!(verify_with_jwks(
            std::str::from_utf8(&tampered).unwrap(),
            "oaiapp_echo",
            "fresh-nonce",
            &keys
        )
        .is_err());
    }
    #[test]
    fn an_issued_registration_survives_an_expired_code_without_registering_another_client() {
        let store = MemoryStore::default();
        let host = new_host_id().unwrap();
        let first = PendingSignIn::start(&store, &host, None).unwrap();
        store
            .set(
                &first.pending_key,
                &Secret::new(serde_json::to_string("oaiapp_previously_issued").unwrap()),
            )
            .unwrap();
        let retry = PendingSignIn::start(&store, &host, None).unwrap();
        let url = Url::parse(retry.authorization_url()).unwrap();
        let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["client_id"], "oaiapp_previously_issued");
        assert!(!pairs.contains_key("agent_name_hint"));
        assert_ne!(first.state.expose(), retry.state.expose());
        assert_ne!(first.verifier.expose(), retry.verifier.expose());
    }
    #[test]
    fn returning_accounts_cannot_replace_another_registration_or_infer_without_grant() {
        let store = MemoryStore::default();
        let session = Session {
            account: Account {
                credential_id: "one".into(),
                subject: "sub-one".into(),
                email: "same@example.com".into(),
                client_id: "oaiapp_one".into(),
                plan_enabled: false,
            },
            access_token: "not-authorized".into(),
            refresh_token: "".into(),
            id_token: "retained-id-token".into(),
            scopes: "openid".into(),
            expires_at: now() + 3600,
        };
        save(&store, &session).unwrap();
        let pending = PendingSignIn::start(&store, &new_host_id().unwrap(), Some("one")).unwrap();
        let url = Url::parse(pending.authorization_url()).unwrap();
        let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["client_id"], "oaiapp_one");
        assert!(pairs.get("agent_name_hint").is_none());
        let mut callback = Url::parse(&pending.redirect).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("code", "code")
            .append_pair("state", pending.state.expose())
            .append_pair("client_id", "oaiapp_other");
        assert!(pending.callback(&callback).is_err());
        assert_eq!(
            access_token(&store, "one", &Cancellation::default()).unwrap_err(),
            AiError::Unauthorized
        );
        assert_eq!(account(&store, "one").unwrap().subject, "sub-one");
    }
}
