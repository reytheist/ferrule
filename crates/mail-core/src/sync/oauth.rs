//! OAuth2 PKCE + localhost loopback browser sign-in and refresh-token exchange
//! for Gmail (`GmailOAuth`) and Outlook (`OutlookOAuth`).

use std::collections::HashMap;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

use crate::auth::{ProviderAuth, ProviderKind};
use crate::error::CoreError;
use crate::ids::AccountId;

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone)]
pub(crate) struct OAuthEndpoints {
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: &'static str,
    pub scope: &'static str,
    pub redirect_host: &'static str,
    pub redirect_path: &'static str,
    pub client_id: String,
    pub login_hint: String,
    pub is_google: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct OAuthTokenBundle {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OAuthProfile {
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    email: Option<String>,
    preferred_username: Option<String>,
    upn: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftGraphMe {
    mail: Option<String>,
    user_principal_name: Option<String>,
}

pub(crate) fn endpoints_for_sign_in(
    provider: ProviderKind,
    client_id: &str,
    tenant: Option<&str>,
    login_hint: Option<&str>,
) -> Result<OAuthEndpoints, CoreError> {
    let client_id = client_id.trim();
    let hint = login_hint.unwrap_or("").trim().to_string();
    match provider {
        ProviderKind::Gmail => {
            if client_id.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "clientId".to_string(),
                    reason: "Google OAuth client ID is required".to_string(),
                });
            }
            Ok(OAuthEndpoints {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                userinfo_url: "https://www.googleapis.com/oauth2/v2/userinfo",
                scope: "https://mail.google.com/ email profile",
                redirect_host: "127.0.0.1",
                redirect_path: "/callback",
                client_id: client_id.to_string(),
                login_hint: hint,
                is_google: true,
            })
        }
        ProviderKind::Outlook => {
            if client_id.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "clientId".to_string(),
                    reason: "Microsoft Entra client ID is required".to_string(),
                });
            }
            let tenant_slug = tenant
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or("common");
            Ok(OAuthEndpoints {
                auth_url: format!(
                    "https://login.microsoftonline.com/{tenant_slug}/oauth2/v2.0/authorize"
                ),
                token_url: format!(
                    "https://login.microsoftonline.com/{tenant_slug}/oauth2/v2.0/token"
                ),
                userinfo_url: "https://graph.microsoft.com/v1.0/me",
                scope: "openid email profile offline_access https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send",
                redirect_host: "localhost",
                redirect_path: "",
                client_id: client_id.to_string(),
                login_hint: hint,
                is_google: false,
            })
        }
        ProviderKind::Imap => Err(CoreError::InvalidInput {
            field: "provider".to_string(),
            reason: "browser sign-in is only supported for Gmail and Outlook".to_string(),
        }),
    }
}

pub(crate) fn endpoints_for(auth: &ProviderAuth) -> Result<OAuthEndpoints, CoreError> {
    match auth {
        ProviderAuth::GmailOAuth { address, client } => {
            endpoints_for_sign_in(ProviderKind::Gmail, &client.client_id, None, Some(address))
        }
        ProviderAuth::OutlookOAuth {
            address,
            client_id,
            tenant,
        } => endpoints_for_sign_in(
            ProviderKind::Outlook,
            client_id,
            Some(tenant),
            Some(address),
        ),
        _ => Err(CoreError::InvalidInput {
            field: "auth".to_string(),
            reason: "browser sign-in is only supported for Gmail OAuth and Outlook OAuth"
                .to_string(),
        }),
    }
}

pub(crate) fn generate_pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn pkce_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub(crate) fn build_authorize_url(
    endpoints: &OAuthEndpoints,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, CoreError> {
    let mut url = Url::parse(&endpoints.auth_url).map_err(|e| CoreError::Internal {
        reason: format!("invalid OAuth authorize URL: {e}"),
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("client_id", &endpoints.client_id);
        pairs.append_pair("redirect_uri", redirect_uri);
        pairs.append_pair("response_type", "code");
        pairs.append_pair("scope", endpoints.scope);
        pairs.append_pair("state", state);
        pairs.append_pair("code_challenge", code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        if !endpoints.login_hint.is_empty() {
            pairs.append_pair("login_hint", &endpoints.login_hint);
        }
        if endpoints.is_google {
            pairs.append_pair("access_type", "offline");
            let prompt = if endpoints.login_hint.is_empty() {
                "select_account consent"
            } else {
                "consent"
            };
            pairs.append_pair("prompt", prompt);
        } else {
            pairs.append_pair("prompt", "select_account");
        }
    }
    Ok(url.into())
}

pub(crate) fn parse_callback_request(
    request_line: &str,
    expected_state: &str,
) -> Result<String, CoreError> {
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| CoreError::Network {
            reason: "malformed OAuth callback request".to_string(),
            retryable: false,
        })?;
    let parsed = Url::parse(&format!("http://127.0.0.1{path}")).map_err(|e| CoreError::Network {
        reason: format!("malformed OAuth callback URL: {e}"),
        retryable: false,
    })?;
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        return Err(CoreError::Network {
            reason: format!("OAuth sign-in declined: {desc}"),
            retryable: false,
        });
    }

    let got_state = params.get("state").map(String::as_str).unwrap_or("");
    if got_state != expected_state {
        return Err(CoreError::Network {
            reason: "OAuth state mismatch; try signing in again".to_string(),
            retryable: false,
        });
    }

    params
        .get("code")
        .filter(|c| !c.is_empty())
        .cloned()
        .ok_or_else(|| CoreError::Network {
            reason: "OAuth callback did not contain an authorization code".to_string(),
            retryable: false,
        })
}

fn callback_html(ok: bool, detail: &str) -> String {
    let (title, accent) = if ok {
        ("Signed in to Galapaghost", "#34C4A4")
    } else {
        ("Sign-in did not finish", "#F16A7A")
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{margin:0;min-height:100vh;display:grid;place-items:center;\
         background:#0f1118;color:#edf0f8;font-family:Inter,system-ui,sans-serif;}}\
         .card{{max-width:420px;padding:28px 32px;border-radius:12px;background:#161923;\
         border:1px solid #272e40;text-align:center;}}h1{{margin:0 0 10px;font-size:18px;\
         color:{accent};}}p{{margin:0;font-size:13px;line-height:1.5;color:#b3bacb;}}</style>\
         </head><body><div class=\"card\"><h1>{title}</h1><p>{detail}</p></div></body></html>"
    )
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    html: &str,
) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Open the provider's sign-in page in the default browser, wait for the localhost
/// callback, and exchange the authorization code for tokens.
pub(crate) async fn run_browser_pkce_flow(
    auth: &ProviderAuth,
    client_secret: Option<&str>,
) -> Result<OAuthTokenBundle, CoreError> {
    let endpoints = endpoints_for(auth)?;
    run_browser_pkce_flow_for_endpoints(&endpoints, client_secret).await
}

pub(crate) async fn run_browser_pkce_flow_for_endpoints(
    endpoints: &OAuthEndpoints,
    client_secret: Option<&str>,
) -> Result<OAuthTokenBundle, CoreError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| CoreError::Network {
            reason: format!("could not bind local OAuth callback port: {e}"),
            retryable: true,
        })?;
    let port = listener
        .local_addr()
        .map_err(|e| CoreError::Internal {
            reason: format!("read callback port: {e}"),
        })?
        .port();

    let redirect_uri = format!(
        "http://{}:{port}{}",
        endpoints.redirect_host, endpoints.redirect_path
    );
    let verifier = generate_pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let state = generate_pkce_verifier();
    let auth_url = build_authorize_url(endpoints, &redirect_uri, &state, &challenge)?;

    open::that(&auth_url).map_err(|e| CoreError::Network {
        reason: format!("could not open system browser for sign-in: {e}"),
        retryable: false,
    })?;

    let code = tokio::time::timeout(CALLBACK_TIMEOUT, async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|e| CoreError::Network {
                reason: format!("OAuth callback accept failed: {e}"),
                retryable: true,
            })?;
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                continue;
            }
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            if first_line.contains("favicon.ico") {
                write_http_response(&mut stream, "404 Not Found", "").await;
                continue;
            }
            match parse_callback_request(first_line, &state) {
                Ok(code) => {
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        &callback_html(
                            true,
                            "You can close this browser tab and return to Galapaghost.",
                        ),
                    )
                    .await;
                    return Ok(code);
                }
                Err(err) => {
                    write_http_response(
                        &mut stream,
                        "400 Bad Request",
                        &callback_html(false, &err.to_string()),
                    )
                    .await;
                    return Err(err);
                }
            }
        }
    })
    .await
    .map_err(|_| CoreError::Network {
        reason: "timed out waiting for browser sign-in (3 minutes)".to_string(),
        retryable: true,
    })??;

    exchange_code(endpoints, &redirect_uri, &code, &verifier, client_secret).await
}

pub(crate) fn email_from_id_token(id_token: &str) -> Option<String> {
    let payload_b64 = id_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let claims: IdTokenClaims = serde_json::from_slice(&decoded).ok()?;
    claims
        .email
        .or(claims.preferred_username)
        .or(claims.upn)
        .map(|e| e.trim().to_string())
        .filter(|e| e.contains('@'))
}

pub(crate) async fn fetch_profile(
    endpoints: &OAuthEndpoints,
    bundle: &OAuthTokenBundle,
) -> Result<OAuthProfile, CoreError> {
    if let Some(email) = bundle.id_token.as_deref().and_then(email_from_id_token) {
        return Ok(OAuthProfile { email });
    }
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| CoreError::Internal {
            reason: format!("build HTTP client: {e}"),
        })?;
    let resp = http
        .get(endpoints.userinfo_url)
        .bearer_auth(&bundle.access_token)
        .send()
        .await
        .map_err(|e| CoreError::Network {
            reason: format!("could not fetch OAuth account profile: {e}"),
            retryable: true,
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(CoreError::Network {
            reason: format!("OAuth profile lookup rejected ({status})"),
            retryable: false,
        });
    }

    if endpoints.is_google {
        let info: GoogleUserInfo = resp.json().await.map_err(|e| CoreError::Network {
            reason: format!("invalid Google profile response: {e}"),
            retryable: false,
        })?;
        let email = info
            .email
            .map(|e| e.trim().to_string())
            .filter(|e| e.contains('@'))
            .ok_or_else(|| CoreError::Network {
                reason: "Google did not return an email address for the selected account"
                    .to_string(),
                retryable: false,
            })?;
        Ok(OAuthProfile { email })
    } else {
        let info: MicrosoftGraphMe = resp.json().await.map_err(|e| CoreError::Network {
            reason: format!("invalid Microsoft profile response: {e}"),
            retryable: false,
        })?;
        let email = info
            .mail
            .or(info.user_principal_name)
            .map(|e| e.trim().to_string())
            .filter(|e| e.contains('@'))
            .ok_or_else(|| CoreError::Network {
                reason: "Microsoft did not return an email address for the selected account"
                    .to_string(),
                retryable: false,
            })?;
        Ok(OAuthProfile { email })
    }
}

async fn exchange_code(
    endpoints: &OAuthEndpoints,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokenBundle, CoreError> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("client_id", &endpoints.client_id),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
        form.push(("client_secret", secret.trim()));
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| CoreError::Internal {
            reason: format!("build HTTP client: {e}"),
        })?;
    let resp = http
        .post(&endpoints.token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| CoreError::Network {
            reason: format!("OAuth token exchange failed: {e}"),
            retryable: true,
        })?;
    let status = resp.status();
    let parsed: TokenEndpointResponse = resp.json().await.map_err(|e| CoreError::Network {
        reason: format!("invalid OAuth token response ({status}): {e}"),
        retryable: false,
    })?;

    if !status.is_success() || parsed.error.is_some() {
        let msg = parsed
            .error_description
            .or(parsed.error)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(CoreError::Network {
            reason: format!("OAuth token exchange rejected: {msg}"),
            retryable: false,
        });
    }

    let access_token = parsed.access_token.ok_or_else(|| CoreError::Network {
        reason: "OAuth provider returned no access_token".to_string(),
        retryable: false,
    })?;

    Ok(OAuthTokenBundle {
        access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
        expires_in_secs: parsed.expires_in.unwrap_or(3600),
    })
}

pub(crate) async fn refresh_access_token(
    account_id: AccountId,
    auth: &ProviderAuth,
    refresh_token: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokenBundle, CoreError> {
    let endpoints = endpoints_for(auth)?;
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("client_id", &endpoints.client_id),
        ("refresh_token", refresh_token),
    ];
    if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
        form.push(("client_secret", secret.trim()));
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| CoreError::Internal {
            reason: format!("build HTTP client: {e}"),
        })?;
    let resp = http
        .post(&endpoints.token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| CoreError::Network {
            reason: format!("OAuth token refresh failed: {e}"),
            retryable: true,
        })?;
    let status = resp.status();
    let parsed: TokenEndpointResponse = resp.json().await.map_err(|e| CoreError::Network {
        reason: format!("invalid OAuth refresh response ({status}): {e}"),
        retryable: false,
    })?;

    if !status.is_success() || parsed.error.is_some() {
        let err_code = parsed.error.as_deref().unwrap_or("");
        let msg = parsed
            .error_description
            .or(parsed.error.clone())
            .unwrap_or_else(|| format!("HTTP {status}"));
        if err_code == "invalid_grant" || status.as_u16() == 400 || status.as_u16() == 401 {
            return Err(CoreError::AuthRequired {
                account_id,
                reason: format!("OAuth session expired ({msg}) — click Sign in again"),
            });
        }
        return Err(CoreError::Network {
            reason: format!("OAuth token refresh failed: {msg}"),
            retryable: true,
        });
    }

    let access_token = parsed.access_token.ok_or_else(|| CoreError::AuthRequired {
        account_id,
        reason: "OAuth refresh returned no access token".to_string(),
    })?;

    Ok(OAuthTokenBundle {
        access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
        expires_in_secs: parsed.expires_in.unwrap_or(3600),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::OAuthClient;

    #[test]
    fn pkce_s256_matches_rfc7636_vector() {
        // RFC 7636 Appendix B test vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(pkce_challenge_s256(verifier), expected);
    }

    #[test]
    fn google_and_outlook_authorize_urls_carry_required_params() {
        let gmail = ProviderAuth::GmailOAuth {
            address: "user@gmail.com".into(),
            client: OAuthClient {
                client_id: "123.apps.googleusercontent.com".into(),
                has_client_secret: false,
            },
        };
        let ep = endpoints_for(&gmail).expect("google endpoints");
        let url = build_authorize_url(&ep, "http://127.0.0.1:43210/callback", "st1", "ch1")
            .expect("url");
        assert!(url.contains("accounts.google.com"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("mail.google.com"));

        let outlook = ProviderAuth::OutlookOAuth {
            address: "user@outlook.com".into(),
            client_id: "00000000-0000-0000-0000-000000000001".into(),
            tenant: "common".into(),
        };
        let ep = endpoints_for(&outlook).expect("outlook endpoints");
        let url = build_authorize_url(&ep, "http://localhost:43210/callback", "st2", "ch2")
            .expect("url");
        assert!(url.contains("login.microsoftonline.com/common"));
        assert!(url.contains("IMAP.AccessAsUser.All"));
        assert!(url.contains("offline_access"));

        // New Gmail sign-in without a pre-entered email forces the Google account chooser.
        let fresh_google = endpoints_for_sign_in(
            ProviderKind::Gmail,
            "123.apps.googleusercontent.com",
            None,
            None,
        )
        .expect("fresh google");
        let fresh_url =
            build_authorize_url(&fresh_google, "http://127.0.0.1:43210/callback", "st3", "ch3")
                .expect("fresh url");
        assert!(fresh_url.contains("prompt=select_account+consent"));
        assert!(!fresh_url.contains("login_hint="));
    }

    #[test]
    fn callback_parser_validates_state_and_errors() {
        let code = parse_callback_request("GET /callback?code=abc123&state=good HTTP/1.1", "good")
            .expect("valid callback");
        assert_eq!(code, "abc123");

        let bad_state =
            parse_callback_request("GET /callback?code=abc123&state=wrong HTTP/1.1", "good");
        assert!(bad_state.is_err());

        let denied = parse_callback_request(
            "GET /callback?error=access_denied&error_description=User+cancelled HTTP/1.1",
            "good",
        );
        assert!(denied.unwrap_err().to_string().contains("User cancelled"));
    }

    #[test]
    fn extracts_email_from_oidc_id_token_payload() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"preferred_username":"user@outlook.com"}"#);
        let fake_jwt = format!("eyJhbGciOiJSUzI1NiJ9.{payload}.sig");
        assert_eq!(
            email_from_id_token(&fake_jwt).as_deref(),
            Some("user@outlook.com")
        );
    }
}
