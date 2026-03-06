use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// HTTP client for the StreamHouse REST API.
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    base_url: String,
}

/// Org-scoped client that adds x-organization-id to every request.
#[derive(Clone)]
pub struct OrgClient {
    inner: HttpClient,
    org_id: String,
}

impl HttpClient {
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn with_org(&self, org_id: &str) -> OrgClient {
        OrgClient {
            inner: self.clone(),
            org_id: org_id.to_string(),
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("GET request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GET {path} returned {status}: {body}");
        }
        resp.json().await.context("JSON decode failed")
    }

    pub async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .context("POST request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST {path} returned {status}: {body_text}");
        }
        resp.json().await.context("JSON decode failed")
    }

    pub async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl OrgClient {
    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.inner.base_url, path);
        let resp = self
            .inner
            .client
            .get(&url)
            .header("x-organization-id", &self.org_id)
            .send()
            .await
            .context("GET request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GET {path} returned {status}: {body}");
        }
        resp.json().await.context("JSON decode failed")
    }

    pub async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{}", self.inner.base_url, path);
        let resp = self
            .inner
            .client
            .post(&url)
            .header("x-organization-id", &self.org_id)
            .json(body)
            .send()
            .await
            .context("POST request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST {path} returned {status}: {body_text}");
        }
        resp.json().await.context("JSON decode failed")
    }

    pub async fn post_json_allow_conflict<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<Option<T>> {
        let url = format!("{}{}", self.inner.base_url, path);
        let resp = self
            .inner
            .client
            .post(&url)
            .header("x-organization-id", &self.org_id)
            .json(body)
            .send()
            .await
            .context("POST request failed")?;
        let status = resp.status();
        if status == reqwest::StatusCode::CONFLICT {
            return Ok(None);
        }
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST {path} returned {status}: {body_text}");
        }
        Ok(Some(resp.json().await.context("JSON decode failed")?))
    }
}
