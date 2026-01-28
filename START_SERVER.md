# Quick Start - Run These Commands

## Terminal 1: Start Server

```bash
cd /Users/gabrielbram/Desktop/streamhouse
source .env.dev
cargo run --release --bin streamhouse-server
```

Wait for this message:
```
StreamHouse server starting on 0.0.0.0:50051
```

## Terminal 2: Run Quick Demo

```bash
cd /Users/gabrielbram/Desktop/streamhouse
./scripts/quick-demo.sh
```

This will:
- Create a topic
- Produce 10 messages
- Show storage locations
- Consume messages
- Display summary

## Or: Wait for E2E Script

The `e2e-full-pipeline.sh` script is still running and building the server in release mode.
It will complete in a few minutes and run everything automatically.

You can check its progress:
```bash
ps aux | grep "e2e-full-pipeline"
```

## Current Status

The e2e script stopped your previous server and is:
1. ✅ Cleaned all data
2. ✅ Verified infrastructure
3. ⏳ Building server in release mode (this takes 1-2 minutes)
4. ⏳ Will then run the complete pipeline

**Recommendation**: Open a new terminal and start the server manually (commands above) to run the quick demo now, or wait for the e2e script to finish.
