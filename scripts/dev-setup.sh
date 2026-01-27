#!/bin/bash
set -e

# StreamHouse Development Environment Setup (Phases 1-7)
# This script sets up the complete StreamHouse stack with observability

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  $1${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"

    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed. Please install Docker first."
        exit 1
    fi
    log_success "Docker found: $(docker --version)"

    if ! command -v docker-compose &> /dev/null; then
        log_error "Docker Compose is not installed. Please install Docker Compose first."
        exit 1
    fi
    log_success "Docker Compose found: $(docker-compose --version)"

    if ! command -v cargo &> /dev/null; then
        log_error "Cargo is not installed. Please install Rust first."
        exit 1
    fi
    log_success "Cargo found: $(cargo --version)"
}

# Start all services
start_services() {
    print_header "Starting Infrastructure Services"

    cd "$PROJECT_ROOT"

    log_info "Starting PostgreSQL, MinIO, Prometheus, Grafana, and Alertmanager..."
    docker-compose -f docker-compose.dev.yml up -d

    log_info "Waiting for services to be ready..."
    sleep 10

    # Check PostgreSQL
    log_info "Checking PostgreSQL..."
    until docker exec streamhouse-postgres pg_isready -U streamhouse &> /dev/null; do
        echo -n "."
        sleep 1
    done
    log_success "PostgreSQL is ready"

    # Check MinIO
    log_info "Checking MinIO..."
    until curl -s http://localhost:9000/minio/health/live &> /dev/null; do
        echo -n "."
        sleep 1
    done
    log_success "MinIO is ready"

    # Check Prometheus
    log_info "Checking Prometheus..."
    until curl -s http://localhost:9090/-/healthy &> /dev/null; do
        echo -n "."
        sleep 1
    done
    log_success "Prometheus is ready"

    # Check Grafana
    log_info "Checking Grafana..."
    until curl -s http://localhost:3000/api/health &> /dev/null; do
        echo -n "."
        sleep 1
    done
    log_success "Grafana is ready"
}

# Display service information
show_services() {
    print_header "Service Information"

    echo -e "${BLUE}PostgreSQL:${NC}"
    echo "  URL: postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"
    echo "  User: streamhouse"
    echo "  Password: streamhouse_dev"
    echo ""

    echo -e "${BLUE}MinIO (S3):${NC}"
    echo "  API: http://localhost:9000"
    echo "  Console: http://localhost:9001"
    echo "  Access Key: minioadmin"
    echo "  Secret Key: minioadmin"
    echo "  Bucket: streamhouse-data"
    echo ""

    echo -e "${BLUE}Prometheus:${NC}"
    echo "  URL: http://localhost:9090"
    echo "  Targets: http://localhost:9090/targets"
    echo "  Alerts: http://localhost:9090/alerts"
    echo ""

    echo -e "${BLUE}Grafana:${NC}"
    echo "  URL: http://localhost:3000"
    echo "  Username: admin"
    echo "  Password: admin"
    echo "  Dashboard: StreamHouse Overview"
    echo ""

    echo -e "${BLUE}Alertmanager:${NC}"
    echo "  URL: http://localhost:9093"
    echo ""

    echo -e "${BLUE}Node Exporter:${NC}"
    echo "  URL: http://localhost:9100/metrics"
    echo ""
}

# Setup environment variables
setup_environment() {
    print_header "Setting Up Environment Variables"

    cat > "$PROJECT_ROOT/.env.dev" <<EOF
# StreamHouse Development Environment
DATABASE_URL=postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata
AWS_REGION=us-east-1
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
S3_ENDPOINT=http://localhost:9000
S3_BUCKET=streamhouse-data
RUST_LOG=info
EOF

    log_success "Environment file created: .env.dev"
    log_info "Source it with: source .env.dev"
}

# Build StreamHouse with metrics
build_streamhouse() {
    print_header "Building StreamHouse with Metrics"

    cd "$PROJECT_ROOT"

    log_info "Building with metrics feature enabled..."
    cargo build --release --features metrics

    log_success "StreamHouse built successfully"
}

# Run database migrations
run_migrations() {
    print_header "Running Database Migrations"

    cd "$PROJECT_ROOT"

    export DATABASE_URL="postgresql://streamhouse:streamhouse_dev@localhost:5432/streamhouse_metadata"

    log_info "Creating metadata tables..."
    # Add your migration command here if you have one
    # For now, we'll let the metadata store create tables on first use

    log_success "Database migrations complete"
}

# Show next steps
show_next_steps() {
    print_header "Setup Complete!"

    echo -e "${GREEN}✓ All services are running${NC}"
    echo ""
    echo -e "${YELLOW}Next Steps:${NC}"
    echo ""
    echo "1. Source environment variables:"
    echo -e "   ${BLUE}source .env.dev${NC}"
    echo ""
    echo "2. Run the complete demo:"
    echo -e "   ${BLUE}./scripts/demo-e2e.sh${NC}"
    echo ""
    echo "3. Or run individual components:"
    echo -e "   ${BLUE}cargo run --release --features metrics --bin streamhouse-agent${NC}"
    echo -e "   ${BLUE}cargo run --release --features metrics --example complete_pipeline${NC}"
    echo ""
    echo "4. View dashboards:"
    echo -e "   ${BLUE}Grafana:${NC} http://localhost:3000 (admin/admin)"
    echo -e "   ${BLUE}Prometheus:${NC} http://localhost:9090"
    echo -e "   ${BLUE}MinIO Console:${NC} http://localhost:9001 (minioadmin/minioadmin)"
    echo ""
    echo "5. Stop services:"
    echo -e "   ${BLUE}docker-compose -f docker-compose.dev.yml down${NC}"
    echo ""
    echo -e "${GREEN}Happy coding! 🚀${NC}"
    echo ""
}

# Main execution
main() {
    print_header "StreamHouse Development Setup (Phases 1-7)"

    check_prerequisites
    start_services
    show_services
    setup_environment

    # Ask if user wants to build
    echo ""
    read -p "Build StreamHouse with metrics? (y/n) " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        build_streamhouse
    else
        log_info "Skipping build. You can build later with: cargo build --release --features metrics"
    fi

    show_next_steps
}

# Run main function
main
