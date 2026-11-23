//! The real [`KeyCheck`]: one `GET` to the fixed key endpoint, with the key as a
//! sensitive bearer header and no body or `Location` ever read
//! (`docs/title-and-menu.md` § Decisions settings §3).

use std::future::Future;
use std::pin::Pin;

use super::shared_runtime;
use crate::decision::{
    is_openrouter_key, verdict_from_status, KeyCheck, KeyVerdict, KEY_CHECK_TIMEOUT, KEY_ENDPOINT,
};

/// The real [`KeyCheck`]: `GET https://api.nano-gpt.com/api/v1/api-keys/current` with
/// `Authorization: Bearer <key>` and a [`KEY_CHECK_TIMEOUT`] bound
/// (`docs/title-and-menu.md` § Decisions settings §3). The URL is fixed — it
/// deliberately ignores `UNMEI_DECISIONS_URL` — and the key never reaches a log
/// line: the only data leaving this module is the request itself.
#[derive(Clone)]
pub struct HttpKeyCheck {
    pub(super) http: reqwest::Client,
}

impl std::fmt::Debug for HttpKeyCheck {
    /// Opaque for the same reason as [`DecisionsClient`]: HTTP-client internals
    /// are configuration, and the key travels in a request header.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpKeyCheck").finish_non_exhaustive()
    }
}

impl HttpKeyCheck {
    /// Build with the 5-second ping timeout.
    pub fn new() -> Self {
        HttpKeyCheck {
            http: reqwest::Client::builder()
                .timeout(KEY_CHECK_TIMEOUT)
                // See [`DecisionsClient::new`]: wire dumps and redirects both
                // have to stay off for the credential to stay safe.
                .connection_verbose(false)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("static reqwest client config always builds"),
        }
    }
}

impl Default for HttpKeyCheck {
    fn default() -> Self {
        HttpKeyCheck::new()
    }
}

impl KeyCheck for HttpKeyCheck {
    fn check<'a>(&'a self, key: &'a str) -> Pin<Box<dyn Future<Output = KeyVerdict> + Send + 'a>> {
        let http = self.http.clone();
        let key = key.to_string();
        // An OpenRouter-format key is never sent (N10): no network at all.
        if is_openrouter_key(&key) {
            return Box::pin(async { KeyVerdict::OpenRouterKey });
        }
        // Run on the shared runtime: `reqwest` needs a reactor, and the app
        // polls this future from Bevy's compute task pool.
        Box::pin(async move {
            shared_runtime()
                .spawn(send_key_check(http, KEY_ENDPOINT, key))
                .await
                .unwrap_or(KeyVerdict::Unreachable)
        })
    }
}

/// Build one key-validation request builder. Kept separate from
/// [`send_key_check`] so a test can inspect the constructed request (the bearer
/// header must be marked sensitive).
pub(super) fn key_check_request(
    http: &reqwest::Client,
    endpoint: &str,
    key: &str,
) -> reqwest::RequestBuilder {
    // `bearer_auth` marks the header sensitive; the body is never read.
    http.get(endpoint).bearer_auth(key)
}

/// One ping: any transport error or non-`2xx`/`401`/`403` status — including a
/// `3xx`, which is never followed — resolves [`KeyVerdict::Unreachable`], never
/// a rejection. The body and any `Location` are never read.
pub(super) async fn send_key_check(
    http: reqwest::Client,
    endpoint: &str,
    key: String,
) -> KeyVerdict {
    let response = key_check_request(&http, endpoint, &key).send().await;
    match response {
        Ok(response) => verdict_from_status(response.status().as_u16()),
        Err(_) => KeyVerdict::Unreachable,
    }
}
