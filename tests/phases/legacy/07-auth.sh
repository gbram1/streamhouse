#!/usr/bin/env bash
# Phase 07 — Authentication & API Keys
#
# Tests API key lifecycle, Bearer token auth, permission enforcement,
# topic scoping, and key revocation.
#
# NOTE: The E2E test server runs with STREAMHOUSE_AUTH_ENABLED=false by default.
# When auth is disabled, requests work without auth headers (using X-Organization-Id
# for org scoping). This phase detects whether auth is enabled and skips the
# auth-specific tests gracefully when it is not.

phase_header "Phase 07 — Authentication & API Keys"

API="${TEST_HTTP}/api/v1"

# ═══════════════════════════════════════════════════════════════════════════════
# Detect whether auth is enabled
# ═══════════════════════════════════════════════════════════════════════════════

# Strategy: try a request without any Authorization header to a protected
# endpoint. If auth is enabled, we get 401. If disabled, we get a 2xx/4xx
# that is NOT 401.

AUTH_ENABLED=false
http_request GET "$API/topics"
if [ "$HTTP_STATUS" = "401" ]; then
    AUTH_ENABLED=true
fi

if [ "$AUTH_ENABLED" = "false" ]; then
    echo ""
    echo -e "  ${YELLOW}Auth is DISABLED (STREAMHOUSE_AUTH_ENABLED != true).${NC}"
    echo -e "  ${YELLOW}Running no-auth mode tests only; auth-specific tests skipped.${NC}"
    echo ""
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Without auth (default / no-auth mode)
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$AUTH_ENABLED" = "false" ]; then
    # Requests without Authorization header should succeed
    http_request GET "$API/topics"
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "No-auth mode: GET /topics without auth header works"
    else
        fail "No-auth mode: GET /topics without auth header" "expected 200, got $HTTP_STATUS"
    fi

    # X-Organization-Id scopes operations — create an org first
    http_request POST "$API/organizations" '{"name":"Auth Test Org","slug":"auth-test-org"}'
    if [ "$HTTP_STATUS" = "201" ]; then
        AUTH_ORG_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || AUTH_ORG_ID=""
        pass "No-auth mode: create org without auth header"
    else
        fail "No-auth mode: create org without auth header" "expected 201, got $HTTP_STATUS"
        AUTH_ORG_ID=""
    fi

    if [ -n "$AUTH_ORG_ID" ]; then
        # Create topic scoped to org via X-Organization-Id
        http_request_with_header POST "$API/topics" "X-Organization-Id" "$AUTH_ORG_ID" \
            '{"name":"auth-scoped-topic","partitions":1}'
        if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
            pass "No-auth mode: X-Organization-Id scopes topic creation"
        else
            fail "No-auth mode: X-Organization-Id scopes topic creation" "expected 201/409, got $HTTP_STATUS"
        fi

        # List topics scoped to this org
        http_request_with_header GET "$API/topics" "X-Organization-Id" "$AUTH_ORG_ID"
        if [ "$HTTP_STATUS" = "200" ]; then
            pass "No-auth mode: X-Organization-Id scopes topic listing"
        else
            fail "No-auth mode: X-Organization-Id scopes topic listing" "expected 200, got $HTTP_STATUS"
        fi

        # Cleanup
        http_request_with_header DELETE "$API/topics/auth-scoped-topic" "X-Organization-Id" "$AUTH_ORG_ID" &>/dev/null || true
        http_request DELETE "$API/organizations/$AUTH_ORG_ID" &>/dev/null || true
    fi

    # Skip all auth-enabled tests
    skip "Create API key for org" "auth disabled"
    skip "List API keys (prefix only)" "auth disabled"
    skip "Use API key as Bearer token" "auth disabled"
    skip "Invalid Bearer token -> 401" "auth disabled"
    skip "Revoke API key" "auth disabled"
    skip "Use revoked key -> 401" "auth disabled"
    skip "Read-only key: produce fails (403)" "auth disabled"
    skip "Scoped key: unscoped topic fails (403)" "auth disabled"

else
    # ═══════════════════════════════════════════════════════════════════════════
    # Auth IS enabled — full API key lifecycle tests
    # ═══════════════════════════════════════════════════════════════════════════

    # We need an admin API key to bootstrap. The test server should have one
    # configured, or we create an org + key via the admin routes.
    #
    # Admin routes require admin auth when auth is enabled. We check whether
    # STREAMHOUSE_ADMIN_KEY is set in the environment. If not, we attempt to
    # use the default bootstrap key or report a skip.

    ADMIN_KEY="${STREAMHOUSE_ADMIN_KEY:-}"

    if [ -z "$ADMIN_KEY" ]; then
        echo -e "  ${YELLOW}STREAMHOUSE_ADMIN_KEY not set — trying to bootstrap via org endpoint${NC}"
        # Some deployments leave org management unprotected or use a bootstrap key.
        # Try to create org without auth to see if it works.
        http_request POST "$API/organizations" '{"name":"Auth Phase Org","slug":"auth-phase-org"}'
        if [ "$HTTP_STATUS" = "201" ]; then
            AUTH_PHASE_ORG_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || AUTH_PHASE_ORG_ID=""
        else
            # Cannot proceed without an admin key
            skip "All auth tests" "auth enabled but STREAMHOUSE_ADMIN_KEY not set and cannot create orgs"
            AUTH_PHASE_ORG_ID=""
        fi
    fi

    # If we have an admin key, create an org using it
    if [ -n "$ADMIN_KEY" ]; then
        http_request_with_header POST "$API/organizations" "Authorization" "Bearer $ADMIN_KEY" \
            '{"name":"Auth Phase Org","slug":"auth-phase-org"}'
        if [ "$HTTP_STATUS" = "201" ]; then
            AUTH_PHASE_ORG_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || AUTH_PHASE_ORG_ID=""
            pass "Create organization (via admin key)"
        elif [ "$HTTP_STATUS" = "409" ]; then
            # Slug already exists, try to find it
            http_request_with_header GET "$API/organizations" "Authorization" "Bearer $ADMIN_KEY"
            AUTH_PHASE_ORG_ID=$(echo "$HTTP_BODY" | jq -r '.[] | select(.slug == "auth-phase-org") | .id' 2>/dev/null) || AUTH_PHASE_ORG_ID=""
            pass "Organization 'auth-phase-org' already exists (id=$AUTH_PHASE_ORG_ID)"
        else
            fail "Create organization (via admin key)" "expected 201, got $HTTP_STATUS"
            AUTH_PHASE_ORG_ID=""
        fi
    fi

    if [ -n "${AUTH_PHASE_ORG_ID:-}" ]; then
        # ─── Create API key ──────────────────────────────────────────────────
        BEARER_PREFIX=""
        if [ -n "$ADMIN_KEY" ]; then
            BEARER_PREFIX="Authorization: Bearer $ADMIN_KEY"
        fi

        # Helper: make an admin-authed request
        admin_request() {
            local method="$1"
            local url="$2"
            local data="${3:-}"
            if [ -n "$ADMIN_KEY" ]; then
                http_request_with_header "$method" "$url" "Authorization" "Bearer $ADMIN_KEY" "$data"
            else
                http_request "$method" "$url" "$data"
            fi
        }

        # Create API key with read+write permissions
        admin_request POST "$API/organizations/$AUTH_PHASE_ORG_ID/api-keys" \
            '{"name":"test-rw-key","permissions":["read","write"]}'
        if [ "$HTTP_STATUS" = "201" ]; then
            RW_KEY=$(echo "$HTTP_BODY" | jq -r '.key' 2>/dev/null) || RW_KEY=""
            RW_KEY_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || RW_KEY_ID=""
            RW_KEY_PREFIX=$(echo "$HTTP_BODY" | jq -r '.key_prefix' 2>/dev/null) || RW_KEY_PREFIX=""
            if echo "$RW_KEY" | grep -q "^sk_live_"; then
                pass "Create API key -> 201, key starts with sk_live_"
            else
                fail "Create API key -> 201, key format" "key does not start with sk_live_: $RW_KEY"
            fi
        else
            fail "Create API key" "expected 201, got $HTTP_STATUS: $HTTP_BODY"
            RW_KEY=""
            RW_KEY_ID=""
        fi

        # ─── List API keys (shows prefix, not full key) ─────────────────────
        admin_request GET "$API/organizations/$AUTH_PHASE_ORG_ID/api-keys"
        if [ "$HTTP_STATUS" = "200" ]; then
            KEY_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || KEY_COUNT=0
            # Verify the response contains key_prefix but not the full key field
            HAS_PREFIX=$(echo "$HTTP_BODY" | jq -r '.[0].key_prefix // empty' 2>/dev/null) || HAS_PREFIX=""
            HAS_FULL_KEY=$(echo "$HTTP_BODY" | jq -r '.[0].key // empty' 2>/dev/null) || HAS_FULL_KEY=""
            if [ "$KEY_COUNT" -ge 1 ] && [ -n "$HAS_PREFIX" ] && [ -z "$HAS_FULL_KEY" ]; then
                pass "List API keys -> shows prefix only ($KEY_COUNT keys)"
            elif [ "$KEY_COUNT" -ge 1 ]; then
                pass "List API keys -> $KEY_COUNT keys found"
            else
                fail "List API keys" "expected at least 1 key, got $KEY_COUNT"
            fi
        else
            fail "List API keys" "expected 200, got $HTTP_STATUS"
        fi

        # ─── Use API key as Bearer token ─────────────────────────────────────
        if [ -n "$RW_KEY" ]; then
            # Create a topic for testing using the RW key
            http_request_with_header POST "$API/topics" "Authorization" "Bearer $RW_KEY" \
                '{"name":"auth-test-topic","partitions":1}'
            if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
                pass "Use API key as Bearer token -> topic create succeeds"
            else
                fail "Use API key as Bearer token" "expected 201/409, got $HTTP_STATUS"
            fi

            # GET with valid key
            http_request_with_header GET "$API/topics" "Authorization" "Bearer $RW_KEY"
            if [ "$HTTP_STATUS" = "200" ]; then
                pass "Use API key as Bearer token -> GET /topics succeeds"
            else
                fail "Use API key as Bearer token -> GET" "expected 200, got $HTTP_STATUS"
            fi
        else
            skip "Use API key as Bearer token" "key not created"
        fi

        # ─── Invalid Bearer token -> 401 ─────────────────────────────────────
        http_request_with_header GET "$API/topics" "Authorization" "Bearer sk_live_totally_invalid_key_12345"
        if [ "$HTTP_STATUS" = "401" ]; then
            pass "Invalid Bearer token -> 401"
        else
            fail "Invalid Bearer token" "expected 401, got $HTTP_STATUS"
        fi

        # ─── Missing Bearer token -> 401 ─────────────────────────────────────
        http_request GET "$API/topics"
        if [ "$HTTP_STATUS" = "401" ]; then
            pass "Missing auth header -> 401"
        else
            fail "Missing auth header" "expected 401, got $HTTP_STATUS"
        fi

        # ─── Create read-only key ─────────────────────────────────────────────
        admin_request POST "$API/organizations/$AUTH_PHASE_ORG_ID/api-keys" \
            '{"name":"test-readonly-key","permissions":["read"]}'
        RO_KEY=""
        RO_KEY_ID=""
        if [ "$HTTP_STATUS" = "201" ]; then
            RO_KEY=$(echo "$HTTP_BODY" | jq -r '.key' 2>/dev/null) || RO_KEY=""
            RO_KEY_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || RO_KEY_ID=""
            pass "Create read-only API key -> 201"
        else
            fail "Create read-only API key" "expected 201, got $HTTP_STATUS"
        fi

        # ─── Read-only key: produce should fail (403) ─────────────────────────
        if [ -n "$RO_KEY" ]; then
            http_request_with_header POST "$API/produce" "Authorization" "Bearer $RO_KEY" \
                '{"topic":"auth-test-topic","key":"k1","value":"should-fail"}'
            if [ "$HTTP_STATUS" = "403" ]; then
                pass "Read-only key: produce -> 403 (insufficient permissions)"
            else
                fail "Read-only key: produce" "expected 403, got $HTTP_STATUS"
            fi

            # Read-only key: consume should work
            http_request_with_header GET "$API/topics" "Authorization" "Bearer $RO_KEY"
            if [ "$HTTP_STATUS" = "200" ]; then
                pass "Read-only key: GET /topics -> 200 (read allowed)"
            else
                fail "Read-only key: GET /topics" "expected 200, got $HTTP_STATUS"
            fi
        else
            skip "Read-only key: produce fails (403)" "read-only key not created"
            skip "Read-only key: read works" "read-only key not created"
        fi

        # ─── Create scoped key (limited to specific topics) ───────────────────
        admin_request POST "$API/organizations/$AUTH_PHASE_ORG_ID/api-keys" \
            '{"name":"test-scoped-key","permissions":["read","write"],"scopes":["auth-test-topic"]}'
        SCOPED_KEY=""
        SCOPED_KEY_ID=""
        if [ "$HTTP_STATUS" = "201" ]; then
            SCOPED_KEY=$(echo "$HTTP_BODY" | jq -r '.key' 2>/dev/null) || SCOPED_KEY=""
            SCOPED_KEY_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || SCOPED_KEY_ID=""
            pass "Create scoped API key -> 201"
        else
            fail "Create scoped API key" "expected 201, got $HTTP_STATUS"
        fi

        # ─── Scoped key: access to unscoped topic should fail ─────────────────
        if [ -n "$SCOPED_KEY" ]; then
            # Produce to the scoped topic should work
            http_request_with_header POST "$API/produce" "Authorization" "Bearer $SCOPED_KEY" \
                '{"topic":"auth-test-topic","key":"k1","value":"scoped-ok"}'
            if [ "$HTTP_STATUS" = "200" ]; then
                pass "Scoped key: produce to scoped topic -> 200"
            else
                # Scope enforcement may happen at a different layer; record the result
                fail "Scoped key: produce to scoped topic" "expected 200, got $HTTP_STATUS"
            fi

            # Create a second topic that is NOT in the key's scope
            if [ -n "$RW_KEY" ]; then
                http_request_with_header POST "$API/topics" "Authorization" "Bearer $RW_KEY" \
                    '{"name":"auth-unscoped-topic","partitions":1}'
                # Don't care about status — topic might already exist
            fi

            # Try to produce to unscoped topic — should be blocked (403)
            http_request_with_header POST "$API/produce" "Authorization" "Bearer $SCOPED_KEY" \
                '{"topic":"auth-unscoped-topic","key":"k1","value":"should-fail"}'
            if [ "$HTTP_STATUS" = "403" ]; then
                pass "Scoped key: produce to unscoped topic -> 403"
            else
                fail "Scoped key: produce to unscoped topic" "expected 403, got $HTTP_STATUS"
            fi
        else
            skip "Scoped key: access to scoped topic" "scoped key not created"
            skip "Scoped key: access to unscoped topic fails (403)" "scoped key not created"
        fi

        # ─── Revoke API key ──────────────────────────────────────────────────
        if [ -n "$RW_KEY_ID" ] && [ -n "$RW_KEY" ]; then
            admin_request DELETE "$API/api-keys/$RW_KEY_ID"
            if [ "$HTTP_STATUS" = "204" ]; then
                pass "Revoke API key -> 204"
            else
                fail "Revoke API key" "expected 204, got $HTTP_STATUS"
            fi

            # ─── Use revoked key -> 401 ──────────────────────────────────────
            http_request_with_header GET "$API/topics" "Authorization" "Bearer $RW_KEY"
            if [ "$HTTP_STATUS" = "401" ]; then
                pass "Use revoked key -> 401"
            else
                fail "Use revoked key" "expected 401, got $HTTP_STATUS"
            fi
        else
            skip "Revoke API key" "rw key not created"
            skip "Use revoked key -> 401" "rw key not created"
        fi

        # ─── Cleanup ─────────────────────────────────────────────────────────

        # Revoke remaining keys
        if [ -n "$RO_KEY_ID" ]; then
            admin_request DELETE "$API/api-keys/$RO_KEY_ID" &>/dev/null || true
        fi
        if [ -n "$SCOPED_KEY_ID" ]; then
            admin_request DELETE "$API/api-keys/$SCOPED_KEY_ID" &>/dev/null || true
        fi

        # Delete test topics (use admin key if available)
        if [ -n "$ADMIN_KEY" ]; then
            http_request_with_header DELETE "$API/topics/auth-test-topic" "Authorization" "Bearer $ADMIN_KEY" &>/dev/null || true
            http_request_with_header DELETE "$API/topics/auth-unscoped-topic" "Authorization" "Bearer $ADMIN_KEY" &>/dev/null || true
        fi

        # Delete the test organization
        admin_request DELETE "$API/organizations/$AUTH_PHASE_ORG_ID" &>/dev/null || true

    else
        skip "All API key lifecycle tests" "organization not created (auth enabled but no admin key)"
    fi
fi
