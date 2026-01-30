//! PostgreSQL Storage Tests
//!
//! These tests require a running PostgreSQL instance.
//! Run with: cargo test -p streamhouse-schema-registry --test storage_postgres_test

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use streamhouse_schema_registry::{PostgresSchemaStorage, Schema, SchemaFormat, SchemaStorage};

    // Helper to create test schema
    fn create_test_schema(subject: &str, version: i32) -> Schema {
        Schema {
            id: 0, // Will be assigned by storage
            subject: subject.to_string(),
            version,
            schema_type: SchemaFormat::Avro,
            schema:
                r#"{"type": "record", "name": "Test", "fields": [{"name": "id", "type": "int"}]}"#
                    .to_string(),
            references: vec![],
            metadata: Default::default(),
        }
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database"]
    async fn test_register_schema_returns_id() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost:5432/streamhouse_test".to_string()
        });

        let pool = PgPool::connect(&database_url).await.unwrap();

        // Run migrations
        sqlx::migrate!("../../streamhouse-metadata/migrations")
            .run(&pool)
            .await
            .unwrap();

        let storage = PostgresSchemaStorage::new(pool);
        let schema = create_test_schema("test-subject", 1);

        let id = storage.register_schema(schema).await.unwrap();
        assert!(id > 0);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database"]
    async fn test_duplicate_schema_returns_same_id() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost:5432/streamhouse_test".to_string()
        });

        let pool = PgPool::connect(&database_url).await.unwrap();
        let storage = PostgresSchemaStorage::new(pool);

        let schema1 = create_test_schema("test-subject-2", 1);
        let schema2 = create_test_schema("test-subject-2", 1);

        let id1 = storage.register_schema(schema1).await.unwrap();
        let id2 = storage.register_schema(schema2).await.unwrap();

        assert_eq!(id1, id2, "Same schema should return same ID");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database"]
    async fn test_get_schema_by_id() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost:5432/streamhouse_test".to_string()
        });

        let pool = PgPool::connect(&database_url).await.unwrap();
        let storage = PostgresSchemaStorage::new(pool);

        let schema = create_test_schema("test-subject-3", 1);
        let id = storage.register_schema(schema.clone()).await.unwrap();

        let retrieved = storage.get_schema_by_id(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().schema, schema.schema);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database"]
    async fn test_get_versions_increments() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost:5432/streamhouse_test".to_string()
        });

        let pool = PgPool::connect(&database_url).await.unwrap();
        let storage = PostgresSchemaStorage::new(pool);

        let subject = "test-subject-4";

        // Register two different schemas for same subject
        let mut schema1 = create_test_schema(subject, 1);
        schema1.schema =
            r#"{"type": "record", "name": "Test", "fields": [{"name": "id", "type": "int"}]}"#
                .to_string();
        storage.register_schema(schema1).await.unwrap();

        let mut schema2 = create_test_schema(subject, 2);
        schema2.schema = r#"{"type": "record", "name": "Test", "fields": [{"name": "id", "type": "int"}, {"name": "name", "type": "string"}]}"#.to_string();
        storage.register_schema(schema2).await.unwrap();

        let versions = storage.get_versions(subject).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions, vec![1, 2]);
    }
}
