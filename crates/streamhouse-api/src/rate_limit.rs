//! Rate Limiting Middleware
//!
//! Tower middleware that enforces per-org and per-API-key request rate limits.
//! Applied after AuthLayer so the authenticated key is available in extensions.
//!
//! Byte-rate checks (produce/consume) are done in the individual handlers
//! because middleware cannot read the request body without consuming it.

use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

use streamhouse_metadata::{MetadataStore, QuotaEnforcer};

use crate::auth::AuthenticatedKey;

/// Rate limit layer — wraps handlers with request-count quota checks.
#[derive(Clone)]
pub struct RateLimitLayer {
    enforcer: Arc<QuotaEnforcer<dyn MetadataStore>>,
}

impl RateLimitLayer {
    pub fn new(enforcer: Arc<QuotaEnforcer<dyn MetadataStore>>) -> Self {
        Self { enforcer }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            enforcer: self.enforcer.clone(),
        }
    }
}

/// Rate limit middleware service.
#[derive(Clone)]
pub struct RateLimitMiddleware<S> {
    inner: S,
    enforcer: Arc<QuotaEnforcer<dyn MetadataStore>>,
}

impl<S> Service<Request> for RateLimitMiddleware<S>
where
    S: Service<Request, Response = Response> + Send + Clone + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let enforcer = self.enforcer.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract authenticated key from request extensions (set by AuthLayer)
            let auth_key = request.extensions().get::<AuthenticatedKey>().cloned();

            // If no auth key, skip rate limiting (unauthenticated requests
            // are handled by AuthLayer or are public endpoints)
            let auth_key = match auth_key {
                Some(key) => key,
                None => return inner.call(request).await,
            };

            // Build a minimal TenantContext for the quota check
            let org_id = &auth_key.organization_id;

            // Look up org and quota from the enforcer's metadata store
            // We construct a lightweight TenantContext inline
            let tenant_ctx = match build_tenant_context(org_id, &enforcer).await {
                Some(ctx) => ctx,
                None => {
                    // Org not found or error — let request through
                    // (auth layer already validated the key)
                    return inner.call(request).await;
                }
            };

            // Build optional ApiKey for per-key limit check
            let api_key_for_quota = auth_key_to_metadata_key(&auth_key);

            // Check request rate
            let check = enforcer
                .check_request(&tenant_ctx, api_key_for_quota.as_ref())
                .await;

            match check {
                Ok(streamhouse_metadata::QuotaCheck::Denied(reason)) => {
                    // Compute retry-after
                    let retry_after_ms = enforcer
                        .throttle_time_ms(&tenant_ctx, api_key_for_quota.as_ref())
                        .await;

                    streamhouse_observability::metrics::RATE_LIMIT_TOTAL
                        .with_label_values(&[org_id, "denied", "rest"])
                        .inc();

                    let body = serde_json::json!({
                        "error": "Rate limit exceeded",
                        "message": reason,
                        "retry_after_ms": retry_after_ms,
                    });

                    let mut response =
                        (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();

                    // Set Retry-After header (in seconds, rounded up)
                    let retry_secs = (retry_after_ms as f64 / 1000.0).ceil() as u64;
                    if retry_secs > 0 {
                        response
                            .headers_mut()
                            .insert("Retry-After", retry_secs.to_string().parse().unwrap());
                    }

                    Ok(response)
                }
                Ok(streamhouse_metadata::QuotaCheck::Warning(_)) => {
                    streamhouse_observability::metrics::RATE_LIMIT_TOTAL
                        .with_label_values(&[org_id, "warning", "rest"])
                        .inc();
                    inner.call(request).await
                }
                _ => {
                    streamhouse_observability::metrics::RATE_LIMIT_TOTAL
                        .with_label_values(&[org_id, "allowed", "rest"])
                        .inc();
                    inner.call(request).await
                }
            }
        })
    }
}

/// Build a TenantContext from an org_id using the enforcer's store.
/// Looks up the real org plan from metadata (not hardcoded Free).
pub async fn build_tenant_context(
    org_id: &str,
    enforcer: &QuotaEnforcer<dyn MetadataStore>,
) -> Option<streamhouse_metadata::tenant::TenantContext> {
    Some(enforcer.resolve_tenant_context(org_id).await)
}

/// Convert an AuthenticatedKey to a metadata ApiKey (for per-key quota checks).
fn auth_key_to_metadata_key(_auth: &AuthenticatedKey) -> Option<streamhouse_metadata::ApiKey> {
    // The AuthenticatedKey doesn't carry rate limit fields,
    // so we can't do per-key checks from the middleware alone.
    // Per-key byte-rate checks are done in handlers where we have the full ApiKey.
    // For the request-rate check in middleware, we skip per-key limits.
    None
}
