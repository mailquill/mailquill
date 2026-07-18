//! Minimal authenticated REST helper shared by the Gmail and Graph providers.

use super::ProviderError;
use serde_json::Value;
use std::time::Duration;
use tokio::time;

const GET_JSON_ATTEMPTS: usize = 3;
const GET_JSON_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct Rest {
    client: reqwest::Client,
    token: String,
}

impl Rest {
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self { client, token }
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    async fn check(res: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        let body = res.text().await.unwrap_or_default();
        let excerpt: String = body.chars().take(300).collect();
        Err(ProviderError::Http {
            status: status.as_u16(),
            body: excerpt,
        })
    }

    pub async fn get_json(&self, url: &str) -> Result<Value, ProviderError> {
        for attempt in 1..=GET_JSON_ATTEMPTS {
            match self.get_json_once(url).await {
                Ok(value) => return Ok(value),
                Err(error) if attempt < GET_JSON_ATTEMPTS && is_retryable_get(&error) => {
                    tracing::debug!(
                        attempt,
                        max_attempts = GET_JSON_ATTEMPTS,
                        error = %error,
                        "provider GET failed transiently; retrying"
                    );
                    time::sleep(retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("GET retry loop always returns")
    }

    async fn get_json_once(&self, url: &str) -> Result<Value, ProviderError> {
        let res = self
            .auth(self.client.get(url))
            .timeout(GET_JSON_TIMEOUT)
            .send()
            .await
            .map_err(wrap)?;
        let bytes = Self::check(res).await?.bytes().await.map_err(wrap)?;
        serde_json::from_slice(&bytes).map_err(|e| {
            let excerpt = String::from_utf8_lossy(&bytes)
                .chars()
                .take(300)
                .collect::<String>();
            ProviderError::Other(format!("json decode failed for {url}: {e}; body={excerpt}"))
        })
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, ProviderError> {
        let res = self.auth(self.client.get(url)).send().await.map_err(wrap)?;
        Ok(Self::check(res)
            .await?
            .bytes()
            .await
            .map_err(wrap)?
            .to_vec())
    }

    pub async fn post_json(&self, url: &str, body: &Value) -> Result<Value, ProviderError> {
        let res = self
            .auth(self.client.post(url))
            .json(body)
            .send()
            .await
            .map_err(wrap)?;
        let res = Self::check(res).await?;
        if res.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        // Some endpoints (Gmail trash) return an empty 200 body.
        let bytes = res.bytes().await.map_err(wrap)?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// POST a raw body with an explicit content type (Graph `sendMail` takes
    /// base64 MIME as `text/plain`).
    pub async fn post_raw(
        &self,
        url: &str,
        body: String,
        content_type: &str,
    ) -> Result<(), ProviderError> {
        let res = self
            .auth(self.client.post(url))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .map_err(wrap)?;
        Self::check(res).await?;
        Ok(())
    }

    pub async fn patch_json(&self, url: &str, body: &Value) -> Result<(), ProviderError> {
        let res = self
            .auth(self.client.patch(url))
            .json(body)
            .send()
            .await
            .map_err(wrap)?;
        Self::check(res).await?;
        Ok(())
    }

    pub async fn delete(&self, url: &str) -> Result<(), ProviderError> {
        let res = self
            .auth(self.client.delete(url))
            .send()
            .await
            .map_err(wrap)?;
        Self::check(res).await?;
        Ok(())
    }
}

fn wrap(e: reqwest::Error) -> ProviderError {
    let category = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_request() {
        "request"
    } else if e.is_body() {
        "body"
    } else if e.is_decode() {
        "decode"
    } else {
        "unknown"
    };
    ProviderError::Transport(format!("category={category} detail={e}"))
}

fn is_retryable_get(error: &ProviderError) -> bool {
    match error {
        ProviderError::Transport(_) => true,
        ProviderError::Http { status, .. } => {
            matches!(status, 408 | 425 | 429) || (500..=599).contains(status)
        }
        ProviderError::Other(message) => message.starts_with("json decode failed"),
        _ => false,
    }
}

fn retry_delay(attempt: usize) -> Duration {
    match attempt {
        1 => Duration::from_millis(250),
        _ => Duration::from_secs(1),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_get, Rest};
    use crate::provider::ProviderError;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn retries_only_safe_transient_get_failures() {
        assert!(is_retryable_get(&ProviderError::Transport(
            "category=timeout".into()
        )));
        assert!(is_retryable_get(&ProviderError::Http {
            status: 429,
            body: String::new(),
        }));
        assert!(is_retryable_get(&ProviderError::Http {
            status: 503,
            body: String::new(),
        }));
        assert!(is_retryable_get(&ProviderError::Other(
            "json decode failed for endpoint".into()
        )));

        assert!(!is_retryable_get(&ProviderError::Http {
            status: 401,
            body: String::new(),
        }));
        assert!(!is_retryable_get(&ProviderError::Http {
            status: 404,
            body: String::new(),
        }));
        assert!(!is_retryable_get(&ProviderError::Other(
            "invalid response".into()
        )));
    }

    #[tokio::test]
    async fn get_json_retries_a_transient_server_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (status, body) in [
                ("503 Service Unavailable", "{}"),
                ("200 OK", r#"{"ok":true}"#),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 2_048];
                let _ = socket.read(&mut request).await.unwrap();
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let value = Rest::new("test-token".to_owned())
            .get_json(&format!("http://{address}/metadata"))
            .await
            .unwrap();

        assert_eq!(value["ok"], true);
        server.await.unwrap();
    }
}
