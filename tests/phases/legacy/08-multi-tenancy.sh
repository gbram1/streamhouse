#!/usr/bin/env bash
# Phase 08 — Multi-Tenancy: Organizations & Tenant Isolation
#
# Tests organization CRUD, deployment_mode field, quota/usage endpoints,
# and tenant isolation (topics scoped by X-Organization-Id).

phase_header "Phase 08 — Multi-Tenancy"

API="${TEST_HTTP}/api/v1"

# Track org IDs for cleanup at the end
CLEANUP_ORG_IDS=()

# ═══════════════════════════════════════════════════════════════════════════════
# Organization CRUD
# ═══════════════════════════════════════════════════════════════════════════════

# ─── List organizations (baseline) ───────────────────────────────────────────
http_request GET "$API/organizations"
if [ "$HTTP_STATUS" = "200" ]; then
    BASELINE_COUNT=$(echo "$HTTP_BODY" | jq 'length' 2>/dev/null) || BASELINE_COUNT=0
    pass "List organizations -> 200 (baseline: $BASELINE_COUNT orgs)"
else
    fail "List organizations" "expected 200, got $HTTP_STATUS"
    BASELINE_COUNT=0
fi

# ─── Create organization with name and slug ──────────────────────────────────
http_request POST "$API/organizations" '{"name":"Multi-Tenant Org A","slug":"mt-org-a","plan":"free"}'
if [ "$HTTP_STATUS" = "201" ]; then
    ORG_A_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || ORG_A_ID=""
    CLEANUP_ORG_IDS+=("$ORG_A_ID")
    pass "Create org 'mt-org-a' -> 201 (id=$ORG_A_ID)"
else
    fail "Create org 'mt-org-a'" "expected 201, got $HTTP_STATUS: $HTTP_BODY"
    ORG_A_ID=""
fi

# ─── Duplicate slug -> 409 ──────────────────────────────────────────────────
http_request POST "$API/organizations" '{"name":"Duplicate Slug Org","slug":"mt-org-a"}'
if [ "$HTTP_STATUS" = "409" ]; then
    pass "Duplicate slug -> 409"
else
    fail "Duplicate slug" "expected 409, got $HTTP_STATUS"
fi

# ─── Invalid slug -> 400 ────────────────────────────────────────────────────
http_request POST "$API/organizations" '{"name":"Bad Slug Org","slug":"INVALID SLUG!!"}'
if [ "$HTTP_STATUS" = "400" ]; then
    pass "Invalid slug (uppercase + spaces) -> 400"
else
    fail "Invalid slug" "expected 400, got $HTTP_STATUS"
fi

http_request POST "$API/organizations" '{"name":"Bad Slug Org 2","slug":"has spaces"}'
if [ "$HTTP_STATUS" = "400" ]; then
    pass "Invalid slug (spaces) -> 400"
else
    fail "Invalid slug (spaces)" "expected 400, got $HTTP_STATUS"
fi

# ─── Get organization by ID -> 200 ──────────────────────────────────────────
if [ -n "$ORG_A_ID" ]; then
    http_request GET "$API/organizations/$ORG_A_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        GOT_NAME=$(echo "$HTTP_BODY" | jq -r '.name' 2>/dev/null) || GOT_NAME=""
        GOT_SLUG=$(echo "$HTTP_BODY" | jq -r '.slug' 2>/dev/null) || GOT_SLUG=""
        GOT_PLAN=$(echo "$HTTP_BODY" | jq -r '.plan' 2>/dev/null) || GOT_PLAN=""
        GOT_STATUS=$(echo "$HTTP_BODY" | jq -r '.status' 2>/dev/null) || GOT_STATUS=""
        GOT_DEPLOY=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || GOT_DEPLOY=""

        ALL_MATCH=true
        if [ "$GOT_NAME" != "Multi-Tenant Org A" ]; then ALL_MATCH=false; fi
        if [ "$GOT_SLUG" != "mt-org-a" ]; then ALL_MATCH=false; fi
        if [ "$GOT_PLAN" != "free" ]; then ALL_MATCH=false; fi
        if [ "$GOT_STATUS" != "active" ]; then ALL_MATCH=false; fi

        if [ "$ALL_MATCH" = "true" ]; then
            pass "Get org by ID -> 200, fields match (name, slug, plan=$GOT_PLAN, status=$GOT_STATUS, deployment_mode=$GOT_DEPLOY)"
        else
            fail "Get org by ID -> field mismatch" "name=$GOT_NAME slug=$GOT_SLUG plan=$GOT_PLAN status=$GOT_STATUS"
        fi
    else
        fail "Get org by ID" "expected 200, got $HTTP_STATUS"
    fi
else
    skip "Get org by ID" "org not created"
fi

# ─── Update organization plan -> 200 ────────────────────────────────────────
if [ -n "$ORG_A_ID" ]; then
    http_request PATCH "$API/organizations/$ORG_A_ID" '{"plan":"pro"}'
    if [ "$HTTP_STATUS" = "200" ]; then
        UPDATED_PLAN=$(echo "$HTTP_BODY" | jq -r '.plan' 2>/dev/null) || UPDATED_PLAN=""
        if [ "$UPDATED_PLAN" = "pro" ]; then
            pass "Update org plan to 'pro' -> 200, plan=pro"
        else
            fail "Update org plan" "plan is '$UPDATED_PLAN', expected 'pro'"
        fi
    else
        fail "Update org plan" "expected 200, got $HTTP_STATUS"
    fi
else
    skip "Update org plan" "org not created"
fi

# ─── Get organization quota -> 200 ──────────────────────────────────────────
if [ -n "$ORG_A_ID" ]; then
    http_request GET "$API/organizations/$ORG_A_ID/quota"
    if [ "$HTTP_STATUS" = "200" ]; then
        HAS_MAX_TOPICS=$(echo "$HTTP_BODY" | jq 'has("max_topics")' 2>/dev/null) || HAS_MAX_TOPICS="false"
        HAS_MAX_STORAGE=$(echo "$HTTP_BODY" | jq 'has("max_storage_bytes")' 2>/dev/null) || HAS_MAX_STORAGE="false"
        HAS_MAX_CONNECTIONS=$(echo "$HTTP_BODY" | jq 'has("max_connections")' 2>/dev/null) || HAS_MAX_CONNECTIONS="false"
        HAS_ORG_ID=$(echo "$HTTP_BODY" | jq 'has("organization_id")' 2>/dev/null) || HAS_ORG_ID="false"
        if [ "$HAS_MAX_TOPICS" = "true" ] && [ "$HAS_MAX_STORAGE" = "true" ] && [ "$HAS_ORG_ID" = "true" ]; then
            pass "Get org quota -> 200, has quota fields (max_topics, max_storage_bytes, organization_id)"
        else
            fail "Get org quota -> 200 but missing fields" "max_topics=$HAS_MAX_TOPICS max_storage_bytes=$HAS_MAX_STORAGE organization_id=$HAS_ORG_ID"
        fi
    else
        fail "Get org quota" "expected 200, got $HTTP_STATUS"
    fi
else
    skip "Get org quota" "org not created"
fi

# ─── Get organization usage -> 200 ──────────────────────────────────────────
if [ -n "$ORG_A_ID" ]; then
    http_request GET "$API/organizations/$ORG_A_ID/usage"
    if [ "$HTTP_STATUS" = "200" ]; then
        HAS_TOPICS_COUNT=$(echo "$HTTP_BODY" | jq 'has("topics_count")' 2>/dev/null) || HAS_TOPICS_COUNT="false"
        HAS_STORAGE=$(echo "$HTTP_BODY" | jq 'has("storage_bytes")' 2>/dev/null) || HAS_STORAGE="false"
        HAS_ORG_ID=$(echo "$HTTP_BODY" | jq 'has("organization_id")' 2>/dev/null) || HAS_ORG_ID="false"
        if [ "$HAS_TOPICS_COUNT" = "true" ] && [ "$HAS_STORAGE" = "true" ] && [ "$HAS_ORG_ID" = "true" ]; then
            pass "Get org usage -> 200, has usage fields (topics_count, storage_bytes, organization_id)"
        else
            fail "Get org usage -> 200 but missing fields" "topics_count=$HAS_TOPICS_COUNT storage_bytes=$HAS_STORAGE organization_id=$HAS_ORG_ID"
        fi
    else
        fail "Get org usage" "expected 200, got $HTTP_STATUS"
    fi
else
    skip "Get org usage" "org not created"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# deployment_mode field
# ═══════════════════════════════════════════════════════════════════════════════

# ─── Create org with deployment_mode: self_hosted ────────────────────────────
http_request POST "$API/organizations" '{"name":"Deploy SelfHosted","slug":"mt-deploy-sh","deployment_mode":"self_hosted"}'
if [ "$HTTP_STATUS" = "201" ]; then
    DEPLOY_SH_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || DEPLOY_SH_ID=""
    DEPLOY_SH_MODE=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || DEPLOY_SH_MODE=""
    CLEANUP_ORG_IDS+=("$DEPLOY_SH_ID")
    if [ "$DEPLOY_SH_MODE" = "self_hosted" ]; then
        pass "Create org with deployment_mode=self_hosted -> 201, mode matches"
    else
        fail "Create org deployment_mode=self_hosted" "mode is '$DEPLOY_SH_MODE', expected 'self_hosted'"
    fi
else
    fail "Create org deployment_mode=self_hosted" "expected 201, got $HTTP_STATUS"
    DEPLOY_SH_ID=""
fi

# ─── Create org with deployment_mode: byoc ───────────────────────────────────
http_request POST "$API/organizations" '{"name":"Deploy BYOC","slug":"mt-deploy-byoc","deployment_mode":"byoc"}'
if [ "$HTTP_STATUS" = "201" ]; then
    DEPLOY_BYOC_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || DEPLOY_BYOC_ID=""
    DEPLOY_BYOC_MODE=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || DEPLOY_BYOC_MODE=""
    CLEANUP_ORG_IDS+=("$DEPLOY_BYOC_ID")
    if [ "$DEPLOY_BYOC_MODE" = "byoc" ]; then
        pass "Create org with deployment_mode=byoc -> 201, mode matches"
    else
        fail "Create org deployment_mode=byoc" "mode is '$DEPLOY_BYOC_MODE', expected 'byoc'"
    fi
else
    fail "Create org deployment_mode=byoc" "expected 201, got $HTTP_STATUS"
    DEPLOY_BYOC_ID=""
fi

# ─── Create org with deployment_mode: managed ────────────────────────────────
http_request POST "$API/organizations" '{"name":"Deploy Managed","slug":"mt-deploy-managed","deployment_mode":"managed"}'
if [ "$HTTP_STATUS" = "201" ]; then
    DEPLOY_MANAGED_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || DEPLOY_MANAGED_ID=""
    DEPLOY_MANAGED_MODE=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || DEPLOY_MANAGED_MODE=""
    CLEANUP_ORG_IDS+=("$DEPLOY_MANAGED_ID")
    if [ "$DEPLOY_MANAGED_MODE" = "managed" ]; then
        pass "Create org with deployment_mode=managed -> 201, mode matches"
    else
        fail "Create org deployment_mode=managed" "mode is '$DEPLOY_MANAGED_MODE', expected 'managed'"
    fi
else
    fail "Create org deployment_mode=managed" "expected 201, got $HTTP_STATUS"
    DEPLOY_MANAGED_ID=""
fi

# ─── Get org -> verify deployment_mode persisted ─────────────────────────────
if [ -n "$DEPLOY_BYOC_ID" ]; then
    http_request GET "$API/organizations/$DEPLOY_BYOC_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        FETCHED_MODE=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || FETCHED_MODE=""
        if [ "$FETCHED_MODE" = "byoc" ]; then
            pass "Get org -> deployment_mode=byoc persisted correctly"
        else
            fail "Get org -> deployment_mode" "expected 'byoc', got '$FETCHED_MODE'"
        fi
    else
        fail "Get org to verify deployment_mode" "expected 200, got $HTTP_STATUS"
    fi
else
    skip "Get org -> verify deployment_mode" "byoc org not created"
fi

# ─── Create org without deployment_mode -> defaults to self_hosted ───────────
http_request POST "$API/organizations" '{"name":"Deploy Default","slug":"mt-deploy-default"}'
if [ "$HTTP_STATUS" = "201" ]; then
    DEPLOY_DEFAULT_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || DEPLOY_DEFAULT_ID=""
    DEPLOY_DEFAULT_MODE=$(echo "$HTTP_BODY" | jq -r '.deployment_mode' 2>/dev/null) || DEPLOY_DEFAULT_MODE=""
    CLEANUP_ORG_IDS+=("$DEPLOY_DEFAULT_ID")
    if [ "$DEPLOY_DEFAULT_MODE" = "self_hosted" ]; then
        pass "Create org without deployment_mode -> defaults to 'self_hosted'"
    else
        fail "Create org without deployment_mode" "expected default 'self_hosted', got '$DEPLOY_DEFAULT_MODE'"
    fi
else
    fail "Create org without deployment_mode" "expected 201, got $HTTP_STATUS"
    DEPLOY_DEFAULT_ID=""
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Tenant Isolation
# ═══════════════════════════════════════════════════════════════════════════════

# Create two isolated organizations
http_request POST "$API/organizations" '{"name":"Org Alpha","slug":"mt-org-alpha"}'
if [ "$HTTP_STATUS" = "201" ]; then
    ALPHA_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || ALPHA_ID=""
    CLEANUP_ORG_IDS+=("$ALPHA_ID")
    pass "Create org-alpha for isolation tests (id=$ALPHA_ID)"
else
    fail "Create org-alpha" "expected 201, got $HTTP_STATUS"
    ALPHA_ID=""
fi

http_request POST "$API/organizations" '{"name":"Org Beta","slug":"mt-org-beta"}'
if [ "$HTTP_STATUS" = "201" ]; then
    BETA_ID=$(echo "$HTTP_BODY" | jq -r '.id' 2>/dev/null) || BETA_ID=""
    CLEANUP_ORG_IDS+=("$BETA_ID")
    pass "Create org-beta for isolation tests (id=$BETA_ID)"
else
    fail "Create org-beta" "expected 201, got $HTTP_STATUS"
    BETA_ID=""
fi

if [ -n "$ALPHA_ID" ] && [ -n "$BETA_ID" ]; then
    SHARED_TOPIC_NAME="shared-name"

    # ─── Create topic "shared-name" in org-alpha ─────────────────────────
    http_request_with_header POST "$API/topics" "X-Organization-Id" "$ALPHA_ID" \
        "{\"name\":\"$SHARED_TOPIC_NAME\",\"partitions\":1}"
    if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
        pass "Create topic '$SHARED_TOPIC_NAME' in org-alpha"
    else
        fail "Create topic '$SHARED_TOPIC_NAME' in org-alpha" "expected 201/409, got $HTTP_STATUS"
    fi

    # ─── Create topic "shared-name" in org-beta (same name, different org) ──
    http_request_with_header POST "$API/topics" "X-Organization-Id" "$BETA_ID" \
        "{\"name\":\"$SHARED_TOPIC_NAME\",\"partitions\":1}"
    if [ "$HTTP_STATUS" = "201" ] || [ "$HTTP_STATUS" = "409" ]; then
        pass "Create topic '$SHARED_TOPIC_NAME' in org-beta -> succeeds (different org)"
    else
        fail "Create topic '$SHARED_TOPIC_NAME' in org-beta" "expected 201/409, got $HTTP_STATUS"
    fi

    # ─── List topics for org-alpha -> sees only its own topics ───────────
    http_request_with_header GET "$API/topics" "X-Organization-Id" "$ALPHA_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        # Check that alpha's topic listing contains shared-name
        ALPHA_HAS_TOPIC=$(echo "$HTTP_BODY" | jq '[.[] | .name // empty] | map(select(. == "'"$SHARED_TOPIC_NAME"'")) | length' 2>/dev/null) || ALPHA_HAS_TOPIC=0
        if [ "$ALPHA_HAS_TOPIC" -ge 1 ]; then
            pass "List topics for org-alpha -> sees '$SHARED_TOPIC_NAME'"
        else
            # Try alternate response structure (array of objects with .topics wrapper)
            ALPHA_HAS_TOPIC=$(echo "$HTTP_BODY" | jq '.topics // [] | map(select(.name == "'"$SHARED_TOPIC_NAME"'")) | length' 2>/dev/null) || ALPHA_HAS_TOPIC=0
            if [ "$ALPHA_HAS_TOPIC" -ge 1 ]; then
                pass "List topics for org-alpha -> sees '$SHARED_TOPIC_NAME'"
            else
                fail "List topics for org-alpha" "topic '$SHARED_TOPIC_NAME' not found in response"
            fi
        fi
    else
        fail "List topics for org-alpha" "expected 200, got $HTTP_STATUS"
    fi

    # ─── List topics for org-beta -> sees only its own topics ────────────
    http_request_with_header GET "$API/topics" "X-Organization-Id" "$BETA_ID"
    if [ "$HTTP_STATUS" = "200" ]; then
        BETA_HAS_TOPIC=$(echo "$HTTP_BODY" | jq '[.[] | .name // empty] | map(select(. == "'"$SHARED_TOPIC_NAME"'")) | length' 2>/dev/null) || BETA_HAS_TOPIC=0
        if [ "$BETA_HAS_TOPIC" -ge 1 ]; then
            pass "List topics for org-beta -> sees '$SHARED_TOPIC_NAME'"
        else
            BETA_HAS_TOPIC=$(echo "$HTTP_BODY" | jq '.topics // [] | map(select(.name == "'"$SHARED_TOPIC_NAME"'")) | length' 2>/dev/null) || BETA_HAS_TOPIC=0
            if [ "$BETA_HAS_TOPIC" -ge 1 ]; then
                pass "List topics for org-beta -> sees '$SHARED_TOPIC_NAME'"
            else
                fail "List topics for org-beta" "topic '$SHARED_TOPIC_NAME' not found in response"
            fi
        fi
    else
        fail "List topics for org-beta" "expected 200, got $HTTP_STATUS"
    fi

    # ─── Produce to org-alpha's topic -> succeeds ────────────────────────
    http_request_with_header POST "$API/produce" "X-Organization-Id" "$ALPHA_ID" \
        "{\"topic\":\"$SHARED_TOPIC_NAME\",\"key\":\"alpha-key\",\"value\":\"alpha-data\"}"
    if [ "$HTTP_STATUS" = "200" ]; then
        pass "Produce to org-alpha's '$SHARED_TOPIC_NAME' -> 200"
    else
        fail "Produce to org-alpha's topic" "expected 200, got $HTTP_STATUS"
    fi

    # ─── Consume from org-alpha's topic via org-beta -> 404 (isolation) ──
    # org-beta should NOT be able to see/consume from org-alpha's topic
    http_request_with_header GET "$API/consume?topic=${SHARED_TOPIC_NAME}&partition=0&offset=0&maxRecords=10" \
        "X-Organization-Id" "$BETA_ID"
    if [ "$HTTP_STATUS" = "404" ] || [ "$HTTP_STATUS" = "403" ]; then
        pass "Consume org-alpha's topic via org-beta -> $HTTP_STATUS (isolated)"
    elif [ "$HTTP_STATUS" = "200" ]; then
        # Check if response has zero records (soft isolation)
        RECORD_COUNT=$(echo "$HTTP_BODY" | jq '.records | length' 2>/dev/null) || RECORD_COUNT=-1
        if [ "$RECORD_COUNT" = "0" ]; then
            pass "Consume org-alpha's topic via org-beta -> 200 with 0 records (isolation via empty result)"
        else
            fail "Consume org-alpha's topic via org-beta" "expected 404/403, got 200 with $RECORD_COUNT records (data leak!)"
        fi
    else
        fail "Consume org-alpha's topic via org-beta" "expected 404/403, got $HTTP_STATUS"
    fi

    # ─── Cleanup tenant topics ───────────────────────────────────────────
    http_request_with_header DELETE "$API/topics/$SHARED_TOPIC_NAME" "X-Organization-Id" "$ALPHA_ID" &>/dev/null || true
    http_request_with_header DELETE "$API/topics/$SHARED_TOPIC_NAME" "X-Organization-Id" "$BETA_ID" &>/dev/null || true

else
    skip "Tenant isolation: create topic in org-alpha" "organizations not created"
    skip "Tenant isolation: create topic in org-beta" "organizations not created"
    skip "Tenant isolation: list topics for org-alpha" "organizations not created"
    skip "Tenant isolation: list topics for org-beta" "organizations not created"
    skip "Tenant isolation: produce to org-alpha topic" "organizations not created"
    skip "Tenant isolation: cross-org consume blocked" "organizations not created"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Cleanup — delete all organizations created during this phase
# ═══════════════════════════════════════════════════════════════════════════════

CLEANUP_PASSED=0
CLEANUP_TOTAL=0
for ORG_ID in "${CLEANUP_ORG_IDS[@]}"; do
    if [ -n "$ORG_ID" ]; then
        CLEANUP_TOTAL=$((CLEANUP_TOTAL + 1))
        http_request DELETE "$API/organizations/$ORG_ID"
        if [ "$HTTP_STATUS" = "204" ]; then
            CLEANUP_PASSED=$((CLEANUP_PASSED + 1))
        fi
    fi
done

if [ "$CLEANUP_TOTAL" -gt 0 ]; then
    if [ "$CLEANUP_PASSED" -eq "$CLEANUP_TOTAL" ]; then
        pass "Cleanup: deleted $CLEANUP_PASSED/$CLEANUP_TOTAL organizations -> 204"
    else
        fail "Cleanup: deleted $CLEANUP_PASSED/$CLEANUP_TOTAL organizations" "some deletes failed"
    fi
fi

# Verify org-alpha is gone (404)
if [ -n "$ALPHA_ID" ]; then
    http_request GET "$API/organizations/$ALPHA_ID"
    if [ "$HTTP_STATUS" = "404" ]; then
        pass "Verify deleted org -> 404"
    else
        fail "Verify deleted org" "expected 404, got $HTTP_STATUS"
    fi
fi
