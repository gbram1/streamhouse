# @streamhouse/cli

StreamHouse CLI (`streamctl`) - Command-line tool for S3-Native Event Streaming.

## Installation

```bash
npm install -g @streamhouse/cli
```

This will download the appropriate binary for your platform (macOS or Linux, x64 or arm64).

## Usage

```bash
# Connect to a StreamHouse server
export STREAMHOUSE_ADDR=http://localhost:9090

# Create a topic
streamhouse topic create orders --partitions 3

# Produce a record
streamhouse produce orders --partition 0 --value '{"amount": 99.99}'

# Consume records
streamhouse consume orders --partition 0 --offset 0

# Manage consumer offsets
streamhouse offset commit --group analytics --topic orders --partition 0 --offset 42

# Authentication
streamhouse auth login --server https://api.streamhouse.dev
streamhouse auth status
streamhouse auth whoami
```

## Supported Platforms

| OS    | Architecture |
|-------|-------------|
| macOS | x64, arm64  |
| Linux | x64, arm64  |

## Building from Source

If a pre-built binary is not available for your platform, you can build from source:

```bash
cargo install --git https://github.com/streamhouse/streamhouse streamhouse-cli
```

## Documentation

For full documentation, visit [https://docs.streamhouse.dev](https://docs.streamhouse.dev).

## License

Apache-2.0
