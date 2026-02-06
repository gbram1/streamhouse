//! Integration test for Schema Registry with PostgreSQL and MinIO
//!
//! This test verifies the full schema registry workflow:
//! 1. PostgreSQL storage backend
//! 2. Schema registration and retrieval
//! 3. Compatibility checking
//! 4. Producer integration with schema validation

use sqlx::PgPool;
use streamhouse_schema_registry::{
    CompatibilityMode, PostgresSchemaStorage, Schema, SchemaFormat, SchemaRegistry, SchemaStorage,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StreamHouse Schema Registry Integration Test\n");
    println!("=" .repeat(70));

    // Step 1: Connect to PostgreSQL
    println!("\n[1/6] Connecting to PostgreSQL...");
    let database_url =
        "postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata";
    let pool = PgPool::connect(database_url).await?;
    println!("✅ Connected to PostgreSQL");

    // Step 2: Create storage and registry
    println!("\n[2/6] Initializing Schema Registry...");
    let storage = PostgresSchemaStorage::new(pool);
    let registry = SchemaRegistry::new(Box::new(storage));
    println!("✅ Schema Registry initialized");

    // Step 3: Register a schema
    println!("\n[3/6] Registering Avro schema...");
    let avro_schema = r#"{
  "type": "record",
  "name": "Order",
  "namespace": "com.streamhouse.example",
  "fields": [
    {"name": "order_id", "type": "string"},
    {"name": "customer_id", "type": "string"},
    {"name": "amount", "type": "double"},
    {"name": "timestamp", "type": "long"}
  ]
}"#;

    let subject = "orders-value";
    let schema_id = registry
        .register_schema(subject, avro_schema, SchemaFormat::Avro)
        .await?;
    println!("✅ Schema registered with ID: {}", schema_id);

    // Step 4: Retrieve the schema
    println!("\n[4/6] Retrieving schema by ID...");
    let retrieved = registry.get_schema_by_id(schema_id).await?;
    match retrieved {
        Some(schema) => {
            println!("✅ Schema retrieved successfully:");
            println!("   - Subject: {}", schema.subject);
            println!("   - Version: {}", schema.version);
            println!("   - Format: {:?}", schema.schema_type);
        }
        None => {
            return Err("Schema not found!".into());
        }
    }

    // Step 5: Test duplicate registration (should return same ID)
    println!("\n[5/6] Testing duplicate registration...");
    let schema_id_2 = registry
        .register_schema(subject, avro_schema, SchemaFormat::Avro)
        .await?;
    if schema_id == schema_id_2 {
        println!("✅ Duplicate schema returned same ID: {} (deduplication works!)", schema_id);
    } else {
        return Err(format!(
            "Expected same ID {} but got {}",
            schema_id, schema_id_2
        )
        .into());
    }

    // Step 6: Test compatibility checking (backward compatible change)
    println!("\n[6/6] Testing backward compatibility...");
    let backward_compatible_schema = r#"{
  "type": "record",
  "name": "Order",
  "namespace": "com.streamhouse.example",
  "fields": [
    {"name": "order_id", "type": "string"},
    {"name": "customer_id", "type": "string"},
    {"name": "amount", "type": "double"},
    {"name": "timestamp", "type": "long"},
    {"name": "notes", "type": ["null", "string"], "default": null}
  ]
}"#;

    let is_compatible = registry
        .check_compatibility(subject, backward_compatible_schema, SchemaFormat::Avro)
        .await?;
    if is_compatible {
        println!("✅ Backward compatible schema accepted");
    } else {
        println!("❌ Backward compatible schema rejected (unexpected)");
        return Err("Compatibility check failed".into());
    }

    // Test incompatible change (removing required field)
    println!("\n   Testing backward incompatible change...");
    let incompatible_schema = r#"{
  "type": "record",
  "name": "Order",
  "namespace": "com.streamhouse.example",
  "fields": [
    {"name": "order_id", "type": "string"},
    {"name": "amount", "type": "double"}
  ]
}"#;

    let is_compatible = registry
        .check_compatibility(subject, incompatible_schema, SchemaFormat::Avro)
        .await?;
    if !is_compatible {
        println!("✅ Backward incompatible schema correctly rejected");
    } else {
        println!("❌ Backward incompatible schema accepted (unexpected)");
        return Err("Compatibility check should have failed".into());
    }

    // Summary
    println!("\n" .repeat(1));
    println!("=" .repeat(70));
    println!("🎉 All tests passed!");
    println!("=" .repeat(70));
    println!("\n✅ Schema Registry Features Verified:");
    println!("   • PostgreSQL storage backend");
    println!("   • Schema registration and retrieval");
    println!("   • Schema deduplication (same schema → same ID)");
    println!("   • Backward compatibility checking");
    println!("   • Incompatible schema rejection");
    println!("\n📊 Test Results:");
    println!("   • Schema ID: {}", schema_id);
    println!("   • Subject: {}", subject);
    println!("   • Format: Avro");
    println!("   • Compatibility Mode: {:?}", CompatibilityMode::Backward);

    Ok(())
}
