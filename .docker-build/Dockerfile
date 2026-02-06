FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 streamhouse

COPY unified-server /app/unified-server
RUN chmod +x /app/unified-server

RUN mkdir -p /data/wal /data/cache && chown -R streamhouse:streamhouse /data

USER streamhouse

ENV RUST_LOG=info HTTP_PORT=8080 GRPC_PORT=9090 WAL_DIR=/data/wal STREAMHOUSE_CACHE=/data/cache

EXPOSE 8080 9090

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["/app/unified-server"]
