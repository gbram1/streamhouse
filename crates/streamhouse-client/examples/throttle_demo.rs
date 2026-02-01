//! Demonstration of S3 throttling protection
//!
//! This example shows how the system handles high write load:
//! 1. Writes succeed up to configured limits
//! 2. Rate limiting kicks in (backpressure)
//! 3. Producers receive RESOURCE_EXHAUSTED errors
//! 4. System recovers gracefully
//!
//! Usage:
//!   cargo run --example throttle_demo

use streamhouse_client::{Producer, ProducerBuilder};
use bytes::Bytes;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 S3 Throttling Protection Demo\n");
    println!("=" .repeat(60));

    // Create producer
    let producer = ProducerBuilder::new()
        .agent_address("http://localhost:50051")
        .build()
        .await?;

    println!("\n✓ Producer connected to agent");
    println!("  Agent should have throttling enabled by default");
    println!("  (THROTTLE_ENABLED=true, rate limits: PUT=3000/s)\n");

    // Send bursts of messages to trigger throttling
    let topic = "throttle-test";
    let mut total_success = 0;
    let mut total_throttled = 0;

    println!("📤 Sending bursts of 50 messages each...\n");

    for batch in 0..10 {
        let start = Instant::now();
        let mut batch_success = 0;
        let mut batch_throttled = 0;

        for i in 0..50 {
            let key = format!("batch-{}-msg-{}", batch, i).into_bytes();
            let value = vec![0u8; 10000]; // 10KB per message

            match producer.send(topic, Some(&key), &value, None).await {
                Ok(_) => {
                    batch_success += 1;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("RESOURCE_EXHAUSTED") || err_str.contains("rate limit") {
                        batch_throttled += 1;
                    } else if err_str.contains("UNAVAILABLE") || err_str.contains("circuit") {
                        println!("  ⚠ Circuit breaker open - backing off");
                        batch_throttled += 1;
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    } else {
                        eprintln!("  ❌ Unexpected error: {}", e);
                    }
                }
            }
        }

        total_success += batch_success;
        total_throttled += batch_throttled;

        let duration = start.elapsed();
        println!(
            "Batch {:2}: {:2} succeeded, {:2} throttled ({:?})",
            batch, batch_success, batch_throttled, duration
        );

        // Small delay between batches to allow token refill
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    println!("\n" .repeat(1));
    println!("=" .repeat(60));
    println!("📊 Final Results:");
    println!("  - Total sent successfully: {}", total_success);
    println!("  - Total throttled: {}", total_throttled);
    println!("  - Throttle rate: {:.1}%", (total_throttled as f64 / (total_success + total_throttled) as f64) * 100.0);

    if total_throttled > 0 {
        println!("\n✅ Throttling protection is WORKING!");
        println!("   The system rejected excess requests to stay within S3 limits.");
    } else {
        println!("\n⚠ No throttling observed");
        println!("   Either limits are high enough, or agent has throttling disabled.");
    }

    Ok(())
}
