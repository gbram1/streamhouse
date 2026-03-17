#!/usr/bin/env bash
# Merge all phase scripts into one e2e-tests.sh
set -euo pipefail

PHASE_DIR="phases"
OUT="e2e-tests.sh"

cat > "$OUT" <<'HEADER'
#!/usr/bin/env bash
# StreamHouse End-to-End Tests
# Combined from individual phase scripts into one comprehensive test suite.
#
# This file is sourced by run-all.sh after server setup.
# It expects common.sh to be already sourced and the server to be running.

API="${TEST_HTTP}/api/v1"
SR="${TEST_HTTP}/schemas"
HEADER

for f in "$PHASE_DIR"/00-full-path.sh \
         "$PHASE_DIR"/01-smoke.sh \
         "$PHASE_DIR"/02-topics.sh \
         "$PHASE_DIR"/03-produce-consume.sh \
         "$PHASE_DIR"/04-schemas.sh \
         "$PHASE_DIR"/05-sql.sh \
         "$PHASE_DIR"/06-consumer-groups.sh \
         "$PHASE_DIR"/07-auth.sh \
         "$PHASE_DIR"/08-multi-tenancy.sh \
         "$PHASE_DIR"/09-connectors.sh \
         "$PHASE_DIR"/10-pipelines.sh \
         "$PHASE_DIR"/11-metrics.sh \
         "$PHASE_DIR"/12-cross-protocol.sh \
         "$PHASE_DIR"/13-negative.sh \
         "$PHASE_DIR"/14-concurrent-load.sh \
         "$PHASE_DIR"/15-cli.sh; do
    [ -f "$f" ] || continue
    echo "" >> "$OUT"
    echo "# $(printf '═%.0s' {1..78})" >> "$OUT"
    # Strip shebang line and leading comments, keep everything else
    sed '1{/^#!/d}' "$f" >> "$OUT"
done

chmod +x "$OUT"
echo "Created $OUT ($(wc -l < "$OUT") lines)"
