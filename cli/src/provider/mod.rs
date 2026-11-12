//! Authoring chat provider: config resolution + the HTTP translation seam.
//!
//! Precedence (author-cli contract §4), highest first: flags > `unmei.author.ron`
//! at the pack root > built-in defaults (base_url `https://openrouter.ai/api`, **no
//! default model**). The API key is **environment-only** — `UNMEI_API_KEY`, then
//! `OPENROUTER_API_KEY`; never a flag or a file, so it can't be committed.
//! A missing key or model errors before any network call.
//!
//! Diagnostics are categorical ([ADR 0014]): a retained error never carries a
//! provider body, URL or the transport/decoder text that produced it, and the
//! credential-bearing values below keep an opaque `Debug`.
//!
//! [ADR 0014]: ../../../docs/adr/0014-cli-diagnostics-and-terminal-credential-safety.md

use std::path::Path;

use serde::Deserialize;
use unmei_engine::read_pack_text;

mod chat;

/// The recognized API-key environment variables, in precedence order: the first
/// non-empty value wins. The CLI masks both values on its terminal streams
/// ([ADR 0014]).
pub const API_KEY_ENVS: [&str; 2] = ["UNMEI_API_KEY", "OPENROUTER_API_KEY"];

pub use chat::OpenAiChat;

/// Built-in endpoint default; any OpenAI-compatible chat endpoint works. The
/// chat path lives under the same `/api` prefix the Decisions endpoint uses.
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";

/// Resolved provider configuration (after precedence).
#[derive(Clone, PartialEq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    /// Resolved from the environment, never a flag or config file.
    pub api_key: String,
}

// Opaque: the resolved key is credential material that must not reach a
// retained diagnostic, and an authored base URL may itself carry userinfo.
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderConfig { .. }")
    }
}

/// Pack-root file `unmei.author.ron` (optional): authoring provider defaults.
///
/// The file is hand-authored RON — a different shape from the env-only key:
///
/// ```ron
/// ( base_url: "https://openrouter.ai/api", model: "openai/gpt-4o-mini" )
/// ```
#[derive(Default, PartialEq)]
pub struct AuthorFile {
    pub base_url: Option<String>,
    pub model: Option<String>,
}

// Opaque like the resolved config: an authored base URL may carry userinfo.
impl std::fmt::Debug for AuthorFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthorFile { .. }")
    }
}

// Option<String> fields authored as plain RON strings or omitted — never RON's
// `Some("...")` wrapping. The engine pack RON bridge owns the one provider
// optional-string conversion; adopting it keeps provider configuration's
// stricter contract separate from the content format's legacy tolerance.

impl<'de> Deserialize<'de> for AuthorFile {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            #[serde(
                default,
                deserialize_with = "unmei_engine::pack_ron::provider_optional_string::deserialize"
            )]
            base_url: Option<String>,
            #[serde(
                default,
                deserialize_with = "unmei_engine::pack_ron::provider_optional_string::deserialize"
            )]
            model: Option<String>,
        }
        let raw = Raw::deserialize(d)?;
        Ok(AuthorFile {
            base_url: raw.base_url,
            model: raw.model,
        })
    }
}

// ---------------------------------------------------------------------------
// Config resolution
// ---------------------------------------------------------------------------

pub struct ConfigInput<'a> {
    pub pack_root: &'a Path,
    /// `--model` flag, when given.
    pub model: Option<&'a str>,
    /// `--base-url` flag, when given.
    pub base_url: Option<&'a str>,
}

/// Resolve provider config with the contract's precedence: flags > pack file >
/// defaults for base_url/model, then the env key. Returns a plain message
/// naming exactly what to set when the key or model is missing (§4: immediate,
/// clear, no network call).
pub fn resolve_provider(input: ConfigInput<'_>) -> Result<ProviderConfig, String> {
    let file_path = input.pack_root.join("unmei.author.ron");
    let file: AuthorFile = match read_pack_text(&file_path) {
        Ok(src) => ron::from_str(&src)
            .map_err(|e| format!("invalid RON in {}: {e}", file_path.display()))?,
        Err(_) => AuthorFile::default(),
    };

    let base_url = input
        .base_url
        .map(str::to_string)
        .or(file.base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    let model = input
        .model
        .map(str::to_string)
        .or(file.model)
        .ok_or_else(|| {
            "no authoring model set — pass --model <id> or set `model` in unmei.author.ron; \
             the authoring model must serve OpenAI chat/completions (decision models like \
             typesafe/jev reject that endpoint)"
                .to_string()
        })?;

    let api_key = API_KEY_ENVS
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|key| !key.is_empty()))
        .ok_or_else(|| {
            "no authoring API key — export UNMEI_API_KEY (or OPENROUTER_API_KEY); \
             keys are environment-only and never accepted from a flag or file"
                .to_string()
        })?;

    Ok(ProviderConfig {
        base_url,
        model,
        api_key,
    })
}

// ---------------------------------------------------------------------------
// The chat seam + OpenAI translation
// ---------------------------------------------------------------------------

/// One chat message. `role` is `system` | `user` | `assistant` (OpenAI wire).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Message {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// A retained provider failure: fixed categorical detail only.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    /// The request produced no response. The transport's own text — URL, OS
    /// error, TLS detail — is deliberately dropped.
    #[error("HTTP request failed: {0}")]
    Http(TransportFailure),
    /// A non-success status. The response body is deliberately not retained.
    #[error("authoring endpoint returned HTTP {status}")]
    Status { status: u16 },
    /// A success response that is not the chat-completions envelope.
    #[error("unexpected response shape from the authoring endpoint: {0}")]
    Shape(ShapeProblem),
    /// The endpoint cannot honour `response_format` (or the `provider`
    /// preferences that route structured requests); drafting falls back to
    /// free text rather than surfacing this (drafting contract §7). The
    /// inspected body is classified, then dropped.
    #[error("the authoring endpoint cannot honour structured output: {reason}")]
    NoStructuredSupport { reason: StructuredUnsupported },
}

/// Why a request produced no response — one fixed category, never the
/// `reqwest` error's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFailure {
    /// No connection was established (refused, DNS, TLS handshake).
    Connect,
    /// The bounded request timeout elapsed.
    Timeout,
    /// Any other transport failure, including a response that ended mid-body.
    Other,
}

impl TransportFailure {
    fn classify(err: &reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else if err.is_connect() {
            Self::Connect
        } else {
            Self::Other
        }
    }
}

impl std::fmt::Display for TransportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Connect => "could not connect to the authoring endpoint",
            Self::Timeout => "the request timed out",
            Self::Other => "transport failure",
        })
    }
}

/// Why a success response could not be decoded — no decoder text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeProblem {
    /// The body was not JSON in the chat-completions envelope.
    NotJson,
    /// The envelope carried no `choices[0].message.content`.
    NoContent,
}

impl std::fmt::Display for ShapeProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotJson => "the reply was not a JSON chat-completions envelope",
            Self::NoContent => "the reply carried no choices[0].message.content",
        })
    }
}

/// The fixed category of a structured-output refusal. Structured support is
/// classified by inspecting the response transiently; only this survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredUnsupported {
    /// The endpoint rejected `response_format`/`json_schema`.
    ResponseFormat,
    /// No endpoint serving this model accepts the requested parameters.
    NoSupportingEndpoint,
    /// The OpenRouter `provider` preferences field was an unknown request
    /// argument, so the endpoint cannot route structured requests.
    ProviderArgument,
    /// The chat seam itself has no structured-output implementation (fakes).
    NotImplemented,
}

impl std::fmt::Display for StructuredUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ResponseFormat => "the endpoint rejected `response_format`",
            Self::NoSupportingEndpoint => {
                "no endpoint serving this model accepts the requested parameters"
            }
            Self::ProviderArgument => "the endpoint rejected the `provider` request argument",
            Self::NotImplemented => "the chat seam has no structured-output implementation",
        })
    }
}

/// A structured-output request: the JSON schema the reply must satisfy. The CLI
/// renders the reply to `.unmei` text itself, so newlines, colons and escaping
/// stay inside JSON strings (drafting contract §7).
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredOutput {
    /// Schema name sent as `json_schema.name`.
    pub name: String,
    /// The JSON Schema sent as `json_schema.schema`.
    pub schema: serde_json::Value,
}

/// The authoring chat provider — the seam that lets offline tests swap the HTTP
/// call for a fake. `OpenAiChat` is the real translation: OpenAI's wire format
/// (chat messages in, `choices[0].message.content` out), against any
/// compatible endpoint.
pub trait AuthorChat {
    fn complete(
        &self,
        model: &str,
        messages: &[Message],
    ) -> impl std::future::Future<Output = Result<String, ProviderError>> + Send;

    /// The primary drafting request (drafting contract §7): ask for a
    /// JSON-schema-constrained reply.
    ///
    /// The default reports no structured-output support, so a fake that only
    /// implements [`AuthorChat::complete`] exercises the automatic free-text
    /// fallback; [`OpenAiChat`] overrides this with the real request.
    fn complete_structured(
        &self,
        _model: &str,
        _messages: &[Message],
        _structured: &StructuredOutput,
    ) -> impl std::future::Future<Output = Result<String, ProviderError>> + Send {
        async {
            Err(ProviderError::NoStructuredSupport {
                reason: StructuredUnsupported::NotImplemented,
            })
        }
    }
}
