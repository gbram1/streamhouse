//! Setup phase: create organizations, topics, schemas, and connectors.

use crate::http_client::HttpClient;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

/// An organization with its API key and topic list.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct OrgContext {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub api_key: Option<String>,
    pub topics: Vec<TopicSpec>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TopicSpec {
    pub name: String,
    pub partitions: u32,
    pub keyed: bool,
    pub schema_subject: Option<String>,
    pub compacted: bool,
}

#[derive(Deserialize)]
struct OrgResponse {
    id: String,
}


/// Run the full setup phase. All operations are idempotent.
pub async fn run_setup(client: &HttpClient) -> Result<Vec<OrgContext>> {
    let org_specs = vec![
        ("FinTech Corp", "loadtest-fintech", "enterprise"),
        ("ShopStream", "loadtest-ecommerce", "pro"),
        ("DataWave", "loadtest-analytics", "free"),
    ];

    let mut orgs = Vec::new();

    for (name, slug, plan) in &org_specs {
        let org = create_org(client, name, slug, plan).await?;
        orgs.push(org);
    }

    // Create topics for each org
    for org in &mut orgs {
        let specs = topic_specs_for_org(&org.slug);
        for spec in &specs {
            create_topic(client, &org.id, spec).await?;
        }
        org.topics = specs;
    }

    // Schema registration is handled by the schema_evolution workload.
    // Registering schemas for topics with schema_subject would enable server-side
    // validation that requires messages to conform to the schema format.

    // Create Kafka-specific topics in the default org (unauthenticated Kafka connections
    // use the default org). These are used by the Kafka wire protocol producers.
    let default_org_id = "00000000-0000-0000-0000-000000000000";
    let kafka_topics = vec![
        TopicSpec { name: "kafka-orders".into(), partitions: 8, keyed: true, schema_subject: None, compacted: false },
        TopicSpec { name: "kafka-market-data".into(), partitions: 16, keyed: true, schema_subject: None, compacted: false },
    ];
    for spec in &kafka_topics {
        create_topic(client, default_org_id, spec).await?;
    }

    tracing::info!(
        orgs = orgs.len(),
        topics = orgs.iter().map(|o| o.topics.len()).sum::<usize>(),
        kafka_topics = kafka_topics.len(),
        "Setup complete"
    );

    Ok(orgs)
}

async fn create_org(client: &HttpClient, name: &str, slug: &str, plan: &str) -> Result<OrgContext> {
    let body = json!({
        "name": name,
        "slug": slug,
        "plan": plan
    });

    let org_id = match client
        .post_json::<OrgResponse>("/api/v1/organizations", &body)
        .await
    {
        Ok(resp) => {
            tracing::info!(org = slug, id = %resp.id, "Created organization");
            resp.id
        }
        Err(e) => {
            // Try to find existing org by listing
            let err_str = e.to_string();
            if err_str.contains("409") || err_str.contains("conflict") || err_str.contains("Conflict") {
                tracing::info!(org = slug, "Organization already exists, looking up");
                find_org_by_slug(client, slug)
                    .await
                    .context("Failed to find existing org")?
            } else {
                return Err(e.context("Failed to create organization"));
            }
        }
    };

    // Create API key (unique name per run so it always succeeds)
    let key_name = format!("loadtest-{slug}-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let api_key = match client
        .post_json::<serde_json::Value>(
            &format!("/api/v1/organizations/{org_id}/api-keys"),
            &json!({
                "name": key_name,
                "permissions": ["read", "write", "admin"]
            }),
        )
        .await
    {
        Ok(resp) => {
            let raw = resp.get("key").and_then(|v| v.as_str()).map(|s| s.to_string());
            if raw.is_some() {
                tracing::info!(org = slug, name = %key_name, "Created API key for Kafka SASL");
            }
            raw
        }
        Err(e) => {
            tracing::warn!(org = slug, err = %e, "Failed to create API key");
            None
        }
    };

    Ok(OrgContext {
        id: org_id,
        name: name.to_string(),
        slug: slug.to_string(),
        api_key,
        topics: vec![],
    })
}

async fn find_org_by_slug(client: &HttpClient, slug: &str) -> Result<String> {
    let orgs: Vec<serde_json::Value> = client.get_json("/api/v1/organizations").await?;
    for org in orgs {
        if org.get("slug").and_then(|v| v.as_str()) == Some(slug) {
            if let Some(id) = org.get("id").and_then(|v| v.as_str()) {
                return Ok(id.to_string());
            }
        }
    }
    anyhow::bail!("Organization with slug '{slug}' not found")
}

fn topic_specs_for_org(org_slug: &str) -> Vec<TopicSpec> {
    match org_slug {
        "loadtest-fintech" => vec![
            TopicSpec { name: "ft-orders".into(), partitions: 8, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ft-transactions".into(), partitions: 16, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ft-audit".into(), partitions: 4, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "ft-user-events".into(), partitions: 8, keyed: true, schema_subject: Some("ft-user-events-value".into()), compacted: false },
            TopicSpec { name: "ft-alerts".into(), partitions: 2, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "ft-market-data".into(), partitions: 32, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ft-compacted".into(), partitions: 4, keyed: true, schema_subject: None, compacted: true },
            TopicSpec { name: "ft-analytics".into(), partitions: 8, keyed: false, schema_subject: None, compacted: false },
        ],
        "loadtest-ecommerce" => vec![
            TopicSpec { name: "ec-products".into(), partitions: 8, keyed: true, schema_subject: Some("ec-products-value".into()), compacted: false },
            TopicSpec { name: "ec-cart-events".into(), partitions: 4, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ec-page-views".into(), partitions: 16, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "ec-inventory".into(), partitions: 4, keyed: true, schema_subject: None, compacted: true },
            TopicSpec { name: "ec-search".into(), partitions: 8, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "ec-recommendations".into(), partitions: 4, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ec-profiles".into(), partitions: 8, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "ec-notifications".into(), partitions: 2, keyed: false, schema_subject: None, compacted: false },
        ],
        "loadtest-analytics" => vec![
            TopicSpec { name: "an-clickstream".into(), partitions: 16, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "an-sessions".into(), partitions: 8, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "an-conversions".into(), partitions: 4, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "an-ab-tests".into(), partitions: 4, keyed: true, schema_subject: None, compacted: false },
            TopicSpec { name: "an-errors".into(), partitions: 8, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "an-metrics-raw".into(), partitions: 16, keyed: false, schema_subject: None, compacted: false },
            TopicSpec { name: "an-metrics-agg".into(), partitions: 4, keyed: true, schema_subject: None, compacted: true },
            TopicSpec { name: "an-user-segments".into(), partitions: 8, keyed: true, schema_subject: Some("an-user-segments-value".into()), compacted: false },
        ],
        _ => vec![],
    }
}

async fn create_topic(client: &HttpClient, org_id: &str, spec: &TopicSpec) -> Result<()> {
    let org_client = client.with_org(org_id);
    let body = json!({
        "name": spec.name,
        "partitions": spec.partitions,
        "replication_factor": 1
    });

    match org_client
        .post_json_allow_conflict::<serde_json::Value>("/api/v1/topics", &body)
        .await
    {
        Ok(Some(_)) => {
            tracing::info!(topic = %spec.name, partitions = spec.partitions, "Created topic");
        }
        Ok(None) => {
            tracing::debug!(topic = %spec.name, "Topic already exists");
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("409") || err_str.contains("already exists") {
                tracing::debug!(topic = %spec.name, "Topic already exists");
            } else {
                return Err(e.context(format!("Failed to create topic {}", spec.name)));
            }
        }
    }
    Ok(())
}

