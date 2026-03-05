#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/target/release/gozzip-sim"
CONFIG="$SCRIPT_DIR/config/realistic-5k.toml"

echo "=== Gozzip Simulator Deploy ==="
echo ""

# Build release binary
echo "Building release binary..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
echo "Binary: $BINARY"
echo "Config: $CONFIG"
echo ""

# Show estimated run parameters
echo "Run parameters (from realistic-5k.toml):"
echo "  Nodes:          5,000"
echo "  BA edges/node:  50 (~50 follows/node)"
echo "  Duration:       30 days"
echo "  Tick interval:  60s"
echo "  Reads/day:      50 per user"
echo "  Est. memory:    4-8 GB (16 GB VPS recommended)"
echo ""

# Optional: deploy to remote host
if [ "${1:-}" = "deploy" ]; then
    REMOTE="${2:?Usage: $0 deploy user@host}"
    REMOTE_DIR="${3:-/opt/gozzip-sim}"

    echo "Deploying to $REMOTE:$REMOTE_DIR ..."
    ssh "$REMOTE" "mkdir -p $REMOTE_DIR/config"
    scp "$BINARY" "$REMOTE:$REMOTE_DIR/gozzip-sim"
    scp "$CONFIG" "$REMOTE:$REMOTE_DIR/config/realistic-5k.toml"

    echo ""
    echo "Deployed. Run on the VPS with:"
    echo "  ssh $REMOTE '$REMOTE_DIR/gozzip-sim --config $REMOTE_DIR/config/realistic-5k.toml validate'"
else
    echo "To deploy to a VPS:"
    echo "  $0 deploy user@host [remote-dir]"
    echo ""
    echo "To run locally:"
    echo "  $BINARY --config $CONFIG validate"
fi
