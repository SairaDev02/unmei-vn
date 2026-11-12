//! The chat-completions wire translation: [`OpenAiChat`] and the request and
//! response shapes it posts. Categorical diagnostics and the credential posture
//! are [ADR 0014]'s; see [`super`] for the resolved configuration.
//!
//! [ADR 0014]: ../../../docs/adr/0014-cli-diagnostics-and-terminal-credential-safety.md

use serde::Deserialize;

use super::{
    AuthorChat, Message, ProviderError, ShapeProblem, StructuredOutput, StructuredUnsupported,
    TransportFailure,
};

/// The real provider: POST `{base_url}/v1/chat/completions`, Bearer key,
/// extract `choices[0].message.content`. Bounded timeout like the Decisions
/// client — a hung endpoint must not hang the CLI.
#[derive(Clone)]
pub struct OpenAiChat {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

// Opaque: this value holds the environment-sourced key.
impl std::fmt::Debug for OpenAiChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenAiChat { .. }")
    }
}

impl OpenAiChat {
    /// Bounded timeout, matching the Decisions layer's posture.
    pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        OpenAiChat {
            http: reqwest::Client::builder()
                .timeout(Self::REQUEST_TIMEOUT)
                // ADR 0014: a redirect would carry the bearer key to a
                // destination the author never named, so none is followed —
                // the 3xx surfaces as an ordinary status failure.
                .redirect(reqwest::redirect::Policy::none())
                // ADR 0014: verbose hyper connection tracing stays off, so a
                // dependency log cannot render the request/response wire. It is
                // reqwest's default; the explicit call records the contract.
                .connection_verbose(false)
                .build()
                .expect("static reqwest client config always builds"),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    fn url(&self) -> String {
        format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        )
    }

    /// The one request shape: `bearer_auth` marks the `Authorization` header
    /// sensitive, so no `Debug` of the request can print the key.
    fn request(&self, body: &ChatRequestBody<'_>) -> reqwest::RequestBuilder {
        self.http
            .post(self.url())
            .bearer_auth(&self.api_key)
            .json(body)
    }

    /// POST one chat request and extract `choices[0].message.content`.
    async fn send(&self, body: &ChatRequestBody<'_>) -> Result<String, WireFailure> {
        let response = self.request(body).send().await.map_err(|e| {
            WireFailure::Provider(ProviderError::Http(TransportFailure::classify(&e)))
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            WireFailure::Provider(ProviderError::Http(TransportFailure::classify(&e)))
        })?;

        if !status.is_success() {
            // The body may carry provider text, a refused URL or the reflected
            // key. It is kept only far enough for the caller's structured-
            // support classification, then dropped (ADR 0014).
            return Err(WireFailure::Rejected {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: ChatResponseBody = serde_json::from_str(&body)
            .map_err(|_| WireFailure::Provider(ProviderError::Shape(ShapeProblem::NotJson)))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or(WireFailure::Provider(ProviderError::Shape(
                ShapeProblem::NoContent,
            )))
    }
}

/// A wire result whose non-success body is still available for classification.
/// This is the transient rejected-response boundary: only [`OpenAiChat`]'s own
/// methods can hold one, and every conversion into a public error drops the
/// body, so a provider payload cannot reach a retained diagnostic.
enum WireFailure {
    Provider(ProviderError),
    Rejected { status: u16, body: String },
}

impl WireFailure {
    /// Convert to the public, body-free error.
    fn into_provider_error(self) -> ProviderError {
        match self {
            WireFailure::Provider(err) => err,
            WireFailure::Rejected { status, .. } => ProviderError::Status { status },
        }
    }
}

/// OpenAI chat wire request (only what the translation needs).
#[derive(serde::Serialize)]
struct ChatRequestBody<'a> {
    model: &'a str,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderPreferences>,
}

/// OpenAI/OpenRouter `response_format` for a `json_schema` request.
#[derive(serde::Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    json_schema: JsonSchemaBody,
}

#[derive(serde::Serialize)]
struct JsonSchemaBody {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}

/// OpenRouter `provider` preferences: `require_parameters: true` routes the
/// request only to endpoints that can honour `response_format`, instead of
/// silently degrading (research 0003 §1, drafting contract §7).
#[derive(serde::Serialize)]
struct ProviderPreferences {
    require_parameters: bool,
}

/// `choices[0].message.content` and nothing else.
#[derive(Deserialize)]
struct ChatResponseBody {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

impl AuthorChat for OpenAiChat {
    async fn complete(&self, model: &str, messages: &[Message]) -> Result<String, ProviderError> {
        let body = ChatRequestBody {
            model,
            messages,
            response_format: None,
            provider: None,
        };
        self.send(&body)
            .await
            .map_err(WireFailure::into_provider_error)
    }

    async fn complete_structured(
        &self,
        model: &str,
        messages: &[Message],
        structured: &StructuredOutput,
    ) -> Result<String, ProviderError> {
        let body = ChatRequestBody {
            model,
            messages,
            response_format: Some(ResponseFormat {
                kind: "json_schema",
                json_schema: JsonSchemaBody {
                    name: structured.name.clone(),
                    strict: true,
                    schema: structured.schema.clone(),
                },
            }),
            provider: Some(ProviderPreferences {
                require_parameters: true,
            }),
        };
        match self.send(&body).await {
            Err(WireFailure::Rejected { status, body }) => {
                match structured_unsupported(status, &body) {
                    Some(reason) => Err(ProviderError::NoStructuredSupport { reason }),
                    None => Err(ProviderError::Status { status }),
                }
            }
            Err(other) => Err(other.into_provider_error()),
            Ok(raw) => Ok(raw),
        }
    }
}

/// HTTP conditions that mean "this endpoint cannot honour structured output" —
/// the one provider condition drafting falls back on instead of surfacing
/// (drafting contract §7). Any other status or body is a hard provider error.
///
/// The body is inspected here and nowhere else; only the returned fixed
/// category survives into [`ProviderError`].
fn structured_unsupported(status: u16, body: &str) -> Option<StructuredUnsupported> {
    if !matches!(status, 400 | 404 | 422) {
        return None;
    }
    let body = body.to_ascii_lowercase();
    if body.contains("response_format")
        || body.contains("json_schema")
        || body.contains("structured output")
        || body.contains("structured_output")
    {
        return Some(StructuredUnsupported::ResponseFormat);
    }
    if body.contains("no endpoints found") && body.contains("support") {
        return Some(StructuredUnsupported::NoSupportingEndpoint);
    }
    // A non-OpenRouter OpenAI-compatible endpoint may reject the OpenRouter
    // `provider` preferences field as an unknown request argument; that is
    // still "no structured support" for our purposes, so it takes the same
    // fallback. Requiring the rejection to name the offending *argument*
    // keeps a general config complaint (e.g. "invalid provider preference")
    // a hard `Status` that surfaces instead of silently downgrading.
    if body.contains("provider")
        && names_request_argument(&body)
        && (body.contains("unknown")
            || body.contains("unrecognized")
            || body.contains("unexpected")
            || body.contains("unsupported")
            || body.contains("not supported")
            || body.contains("invalid"))
    {
        return Some(StructuredUnsupported::ProviderArgument);
    }
    None
}

/// Whether a provider error body is specifically about a request argument
/// (parameter/field), as opposed to a general configuration complaint.
fn names_request_argument(body: &str) -> bool {
    body.contains("request argument")
        || body.contains("parameter")
        || body.contains("argument")
        || body.contains("field")
        || body.contains("property")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR 0014: the bearer header stays sensitive, so neither a `Debug` of the
    /// built request nor a dependency that formats one can print the key.
    #[test]
    fn authorization_header_is_sensitive_and_absent_from_request_debug() {
        let chat = OpenAiChat::new("http://127.0.0.1:1", "dummy-key-abc123");
        let messages = [Message::user("u")];
        let request = chat
            .request(&ChatRequestBody {
                model: "author/m1",
                messages: &messages,
                response_format: None,
                provider: None,
            })
            .build()
            .expect("the static request builds");

        let header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("the request carries the bearer key");
        assert!(
            header.is_sensitive(),
            "the bearer header must stay sensitive"
        );
        assert_eq!(header.to_str().unwrap(), "Bearer dummy-key-abc123");

        let debug = format!("{request:?}");
        assert!(!debug.contains("dummy-key-abc123"), "{debug}");
        assert!(
            debug.to_ascii_lowercase().contains("sensitive"),
            "the header renders as sensitive: {debug}"
        );
    }
}
