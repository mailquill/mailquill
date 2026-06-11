//! Minimal authenticated REST helper shared by the Gmail and Graph providers.

use super::ProviderError;
use serde_json::Value;
use std::time::Duration;

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
        Err(ProviderError::Other(format!("http {status}: {excerpt}")))
    }

    pub async fn get_json(&self, url: &str) -> Result<Value, ProviderError> {
        let res = self.auth(self.client.get(url)).send().await.map_err(wrap)?;
        Ok(Self::check(res).await?.json().await.map_err(wrap)?)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, ProviderError> {
        let res = self.auth(self.client.get(url)).send().await.map_err(wrap)?;
        Ok(Self::check(res).await?.bytes().await.map_err(wrap)?.to_vec())
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
        let res = self.auth(self.client.delete(url)).send().await.map_err(wrap)?;
        Self::check(res).await?;
        Ok(())
    }
}

fn wrap(e: reqwest::Error) -> ProviderError {
    ProviderError::Other(e.to_string())
}
