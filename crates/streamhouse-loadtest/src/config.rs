use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(name = "loadtest", about = "StreamHouse production load test")]
pub struct Config {
    /// HTTP REST API address
    #[arg(long, default_value = "http://localhost:8080", env = "LOADTEST_HTTP_ADDR")]
    pub http_addr: String,

    /// gRPC API address
    #[arg(long, default_value = "http://localhost:50051", env = "LOADTEST_GRPC_ADDR")]
    pub grpc_addr: String,

    /// Kafka protocol address
    #[arg(long, default_value = "127.0.0.1:9092", env = "LOADTEST_KAFKA_ADDR")]
    pub kafka_addr: String,

    /// Prometheus metrics port
    #[arg(long, default_value_t = 9100, env = "LOADTEST_METRICS_PORT")]
    pub metrics_port: u16,

    /// Number of organizations to create
    #[arg(long, default_value_t = 3)]
    pub orgs: usize,

    /// Topics per organization
    #[arg(long, default_value_t = 8)]
    pub topics_per_org: usize,

    /// Target messages/second per producer
    #[arg(long, default_value_t = 100)]
    pub produce_rate: usize,

    /// Records per batch produce
    #[arg(long, default_value_t = 50)]
    pub batch_size: usize,

    /// Duration in seconds (omit for infinite)
    #[arg(long)]
    pub duration: Option<u64>,

    /// Stats reporting interval in seconds
    #[arg(long, default_value_t = 10)]
    pub stats_interval: u64,

    /// SQL query interval in seconds
    #[arg(long, default_value_t = 30)]
    pub sql_interval: u64,

    /// Schema evolution interval in seconds
    #[arg(long, default_value_t = 300)]
    pub schema_evolution_interval: u64,

    /// Integrity check interval in seconds
    #[arg(long, default_value_t = 60)]
    pub integrity_interval: u64,

    /// Storage check interval in seconds
    #[arg(long, default_value_t = 120)]
    pub storage_check_interval: u64,
}
