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

use crate::auth::ProviderAuth;
use crate::error::CoreError;
use crate::ids::AccountId;

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone)]
pub(crate) struct OAuthEndpoints {
    pub auth_url: String,
    pub token_url: String,
    pub scope: &'static str,
    pub redirect_host: &'static str,
    pub client_id: String,
    pub login_hint: String,
    pub is_google: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct OAuthTokenBundle {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in_secs: i64,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

pub(crate) fn endpoints_for(auth: &ProviderAuth) -> Result<OAuthEndpoints, CoreError> {
    match auth {
        ProviderAuth::GmailOAuth { address, client } => {
            if client.client_id.trim().is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "clientId".to_string(),
                    reason: "Google OAuth client ID is required".to_string(),
                });
            }
            Ok(OAuthEndpoints {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scope: "https://mail.google.com/ email profile",
                redirect_host: "127.0.0.1",
                client_id: client.client_id.trim().to_string(),
                login_hint: address.trim().to_string(),
                is_google: true,
            })
        }
        ProviderAuth::OutlookOAuth {
            address,
            client_id,
            tenant,
        } => {
            if client_id.trim().is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "clientId".to_string(),
                    reason: "Microsoft Entra client ID is required".to_string(),
                });
            }
            let tenant_slug = if tenant.trim().is_empty() {
                "common"
            } else {
                tenant.trim()
            };
            Ok(OAuthEndpoints {
                auth_url: format!(
                    "https://login.microsoftonline.com/{tenant_slug}/oauth2/v2.0/authorize"
                ),
                token_url: format!(
                    "https://login.microsoftonline.com/{tenant_slug}/oauth2/v2.0/token"
                ),
                scope: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access User.Read",
                redirect_host: "localhost",
                client_id: client_id.trim().to_string(),
                login_hint: address.trim().to_string(),
                is_google: false,
            })
        }
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
            pairs.append_pair("prompt", "consent");
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

    let redirect_uri = format!("http://{}:{port}/callback", endpoints.redirect_host);
    let verifier = generate_pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let state = generate_pkce_verifier();
    let auth_url = build_authorize_url(&endpoints, &redirect_uri, &state, &challenge)?;

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

    exchange_code(&endpoints, &redirect_uri, &code, &verifier, client_secret).await
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
}
