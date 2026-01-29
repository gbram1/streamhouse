# StreamHouse: Decision Framework

**Purpose:** Guide for deciding whether to pursue StreamHouse as a commercial venture

**Last Updated:** January 29, 2026

---

## The Core Question

**"Should I invest 2+ years of my life building this into a company?"**

This document provides a structured framework to answer that question based on evidence, not hope.

---

## Decision Tree: Is This a Company?

```
START: Do I want to build a company (vs learning project)?
├─ No  → Continue as OSS side project ✅
└─ Yes → Continue ↓

Is there a large enough market ($100M+ TAM)?
├─ No  → Reconsider or find different angle ❌
└─ Yes → Continue ↓
         (Kafka market ~$15B, we target 10-15% = $1.5-2B TAM)

Can I demonstrate 10x better on one dimension?
├─ No  → Unlikely to win against incumbents ❌
└─ Yes → Continue ↓
         (We're 3-5x cheaper, not 10x faster)

Am I willing to compete on cost (not features)?
├─ No  → Find different differentiation ❌
└─ Yes → Continue ↓

Can I achieve profitability at <100 customers?
├─ No  → Need different model (higher ACV) ⚠️
└─ Yes → Continue ↓
         (60 customers @ $12K = $720K ARR = profitable)

Do I have 12-18 months runway (funding or savings)?
├─ No  → Raise pre-seed or save more ⚠️
└─ Yes → Continue ↓

Am I willing to do sales/support (not just code)?
├─ No  → Find co-founder for GTM ⚠️
└─ Yes → Continue ↓

Can I find 3 design partners in next 60 days?
├─ No  → Weak signal, reconsider ❌
└─ Yes → PROCEED ✅

Can I prove 99.9% reliability in POC?
├─ No  → Fix technical issues first ❌
└─ Yes → PROCEED ✅

Do customers say "we'd pay for this"?
├─ No  → Pivot to different value prop ❌
└─ Yes → PROCEED ✅

DECISION: Pursue as company
```

---

## Validation Checklist

Use this checklist to gather evidence before committing.

### Phase 1: Customer Discovery (Weeks 1-4)

**Goal:** Validate that the problem exists and our solution is compelling

```
Customer Interviews (Target: 20)
┌────────────────────────────────────────────────────────────┐
│                                                            │
│  Segment 1: High Kafka Bills (>$5K/month)                  │
│  ☐ Interview 8 companies                                  │
│  ☐ Validate: "Our bill is painful" (6+/10 pain score)     │
│  ☐ Validate: "60% savings justify migration risk"         │
│  ☐ Validate: "We'd try archive tier (low risk)"           │
│  ☐ Find: 2 design partner candidates                      │
│                                                            │
│  Segment 2: Building New Pipelines                         │
│  ☐ Interview 6 companies                                  │
│  ☐ Validate: "We need streaming for analytics"            │
│  ☐ Validate: "Cost is a primary concern"                  │
│  ☐ Validate: "20-50ms latency acceptable"                 │
│  ☐ Find: 1 design partner candidate                       │
│                                                            │
│  Segment 3: Compliance/Archival                            │
│  ☐ Interview 6 companies                                  │
│  ☐ Validate: "Long-term retention required (90+ days)"    │
│  ☐ Validate: "Current solution too expensive"             │
│  ☐ Validate: "Immutability/audit trail important"         │
│  ☐ Find: 1 design partner candidate                       │
│                                                            │
│  Success Criteria:                                         │
│  ✅ 15+/20 say problem is real and painful                │
│  ✅ 10+/20 say our solution is compelling                 │
│  ✅ 4+ design partner candidates identified               │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

**Key Questions to Ask:**

1. "Walk me through your current Kafka setup and costs"
2. "What's your biggest pain point with Kafka?"
3. "If I could save you 60% but with 20-50ms latency, would you consider it?"
4. "What would need to be true for you to try a new streaming platform?"
5. "Would you be willing to pilot an 'archive tier' that doesn't replace Kafka?"

**Red Flags (Stop signals):**
- ❌ "Kafka works fine, don't care about cost"
- ❌ "We need <10ms latency for all workloads"
- ❌ "We'd never use a product without Confluent support"
- ❌ "Migration risk is not worth any savings"

**Green Lights (Continue signals):**
- ✅ "Our Kafka bill is out of control"
- ✅ "We archive to S3 anyway, this could simplify that"
- ✅ "Analytics pipelines don't need low latency"
- ✅ "We'd pilot if you can prove reliability"

### Phase 2: Technical Validation (Weeks 3-6)

**Goal:** Prove technical feasibility and reliability

```
Chaos Engineering Tests
┌────────────────────────────────────────────────────────────┐
│                                                            │
│  Failure Injection                                         │
│  ☐ S3 throttling (reduce rate limits to 100 req/s)        │
│     Expected: Graceful backpressure, no data loss         │
│     Result: _________________                             │
│                                                            │
│  ☐ S3 complete outage (5 minutes)                          │
│     Expected: Write to WAL, replay when S3 returns        │
│     Result: _________________                             │
│                                                            │
│  ☐ PostgreSQL failover (primary → replica)                │
│     Expected: <10s downtime, no data loss                 │
│     Result: _________________                             │
│                                                            │
│  ☐ Network partition (split unified server)               │
│     Expected: Prevent split-brain writes                  │
│     Result: _________________                             │
│                                                            │
│  ☐ Producer overwhelming writer (10x burst)               │
│     Expected: Backpressure, no OOM                        │
│     Result: _________________                             │
│                                                            │
│  Performance Benchmarks                                    │
│  ☐ 10TB dataset, 100M messages/day, 7 days                │
│     Produce p99: _______ ms (target: <20ms)               │
│     Consume p99 (hot): _______ ms (target: <20ms)         │
│     Consume p99 (cold): _______ ms (target: <200ms)       │
│     Uptime: _______ % (target: >99.9%)                    │
│                                                            │
│  Cost Validation                                           │
│  ☐ Actual AWS bill for 10TB:                              │
│     Compute: $_______                                     │
│     S3 storage: $_______                                  │
│     S3 requests: $_______                                 │
│     PostgreSQL: $_______                                  │
│     Network: $_______                                     │
│     Total: $_______ (target: <$600)                       │
│                                                            │
│  Success Criteria:                                         │
│  ✅ All failure tests pass (graceful degradation)         │
│  ✅ p99 latency within targets                            │
│  ✅ 99.9%+ uptime achieved                                │
│  ✅ Cost savings >60% validated                           │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

### Phase 3: Design Partner POC (Weeks 5-12)

**Goal:** Prove product-market fit with real customers

```
Design Partner Program
┌────────────────────────────────────────────────────────────┐
│                                                            │
│  Partner 1: _____________________ (e-commerce)             │
│  ☐ Deployed archive tier connector                        │
│  ☐ Running for 30 days                                    │
│  ☐ Uptime: _______ % (target: >99.9%)                     │
│  ☐ Cost savings: _______ % (target: >60%)                 │
│  ☐ Feedback: _________________________________            │
│  ☐ Would pay? ☐ Yes ☐ No ☐ Maybe                          │
│  ☐ Would recommend? ☐ Yes ☐ No ☐ Maybe                    │
│                                                            │
│  Partner 2: _____________________ (fintech)                │
│  ☐ Deployed archive tier connector                        │
│  ☐ Running for 30 days                                    │
│  ☐ Uptime: _______ % (target: >99.9%)                     │
│  ☐ Cost savings: _______ % (target: >60%)                 │
│  ☐ Feedback: _________________________________            │
│  ☐ Would pay? ☐ Yes ☐ No ☐ Maybe                          │
│  ☐ Would recommend? ☐ Yes ☐ No ☐ Maybe                    │
│                                                            │
│  Partner 3: _____________________ (SaaS)                   │
│  ☐ Deployed archive tier connector                        │
│  ☐ Running for 30 days                                    │
│  ☐ Uptime: _______ % (target: >99.9%)                     │
│  ☐ Cost savings: _______ % (target: >60%)                 │
│  ☐ Feedback: _________________________________            │
│  ☐ Would pay? ☐ Yes ☐ No ☐ Maybe                          │
│  ☐ Would recommend? ☐ Yes ☐ No ☐ Maybe                    │
│                                                            │
│  Success Criteria:                                         │
│  ✅ 3/3 partners successfully deployed                    │
│  ✅ 99.9%+ average uptime across all                      │
│  ✅ 60%+ average cost savings                             │
│  ✅ 2+/3 would pay for product                            │
│  ✅ 2+/3 would recommend to peers                         │
│  ✅ 0 data loss incidents                                 │
│  ✅ <5 support hours/week average                         │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

---

## Go/No-Go Decision Matrix

After completing all validation phases (12 weeks), use this matrix to decide.

### Scoring System

For each criterion, score 0-10 (0 = fail, 10 = excellent)

| Criterion | Score (0-10) | Weight | Weighted |
|-----------|--------------|--------|----------|
| **Customer Demand** | | | |
| Pain point validated (interviews) | __ / 10 | 3x | __ |
| Willing to pay (design partners) | __ / 10 | 3x | __ |
| Design partner success | __ / 10 | 2x | __ |
| **Technical Feasibility** | | | |
| Reliability proven (uptime) | __ / 10 | 3x | __ |
| Cost savings validated | __ / 10 | 2x | __ |
| Performance acceptable | __ / 10 | 2x | __ |
| **Market Position** | | | |
| Differentiation vs Kafka tiered | __ / 10 | 2x | __ |
| TAM size (addressable market) | __ / 10 | 2x | __ |
| Competitive moat | __ / 10 | 1x | __ |
| **Execution Capability** | | | |
| Funding secured/available | __ / 10 | 2x | __ |
| Co-founder recruited | __ / 10 | 2x | __ |
| Founder commitment | __ / 10 | 2x | __ |
| | | **Total:** | __ / 260 |

### Decision Criteria

```
Total Score Interpretation:

200-260: STRONG GO ✅
├─ High confidence in success
├─ Validated demand, proven tech, clear path
└─ Action: Raise seed, go full-time

150-199: QUALIFIED GO ⚠️
├─ Good potential but some gaps
├─ Need to de-risk specific areas
└─ Action: 3-6 month focused improvement, then reassess

100-149: PIVOT 🔄
├─ Core idea has merit but current approach isn't working
├─ Find different wedge or positioning
└─ Action: Customer discovery iteration, test new hypotheses

0-99: NO-GO ❌
├─ Fundamental issues (no demand, tech doesn't work, etc.)
├─ Time to move on or dramatically change direction
└─ Action: Shelve or pivot to completely different product
```

---

## Scenario Planning

### Best Case Scenario

**Month 18:**
- 100 customers @ $12K average = $1.2M ARR
- 25% MoM growth
- 99.95% uptime track record
- 5 customers >$50K/year (logo credibility)
- Raised $3M seed at $20M valuation
- Team of 8 people
- Clear path to $10M ARR in 24 months

**Probability:** 15%

**Required for this outcome:**
- Everything goes right with POCs
- Kafka tiered storage doesn't improve quickly
- We nail the messaging and positioning
- Co-founder is exceptional at sales
- No major technical setbacks

### Base Case Scenario

**Month 18:**
- 40 customers @ $8K average = $320K ARR
- 15% MoM growth
- 99.9% uptime with occasional issues
- 2 customers >$50K/year
- Raised $1.5M seed at $8M valuation OR bootstrapped
- Team of 4 people
- Path to profitability at $1M ARR

**Probability:** 50%

**Required for this outcome:**
- POCs mostly successful
- We find a viable niche (archive tier, multi-cloud)
- Steady execution, no disasters
- Managed service reduces ops burden
- Word-of-mouth drives some growth

### Worst Case Scenario

**Month 18:**
- 5 customers @ $5K average = $25K ARR
- Flat/declining growth
- Multiple reliability incidents
- High churn (>60% annually)
- No funding raised (VCs passed)
- Founder only or 1 employee
- Decision: Shut down or maintain as lifestyle business

**Probability:** 35%

**Reasons this could happen:**
- POCs reveal reliability issues we can't solve
- Kafka tiered storage becomes good enough
- Customers won't migrate (risk > savings)
- We can't hire/raise (recession, market conditions)
- Technical debt compounds

---

## Commitment Framework

### What "Going Full-Time" Means

**Time Investment:**
- 60-80 hours/week for first 12 months
- Weekends and evenings common
- Always on-call (you're the support team)

**Financial Investment:**
- Opportunity cost: $150-300K/year (senior engineer salary)
- Living expenses: $50-100K/year (reduce burn rate)
- Runway needed: 12-18 months ($100-300K savings)

**Emotional Investment:**
- High stress (customer incidents, fundraising, hiring)
- Isolation (especially if solo founder)
- Uncertainty (revenue, product-market fit)
- Relationship strain (family, friends)

**Upside:**
- Potential $10M+ outcome if successful (5-10% equity worth $500K-1M+)
- Learning & growth (technical, business, leadership)
- Network effects (customers, investors, partners)
- Optionality (acquihire, pivot, advisor roles)

### Exit Criteria (When to Stop)

Set these boundaries upfront to avoid sunk cost fallacy:

**Time-based:**
- [ ] If not at $100K ARR by month 12, reassess
- [ ] If not at $500K ARR by month 18, wind down or pivot
- [ ] If not at $2M ARR by month 24, accept lifestyle business or shut down

**Customer-based:**
- [ ] If can't find 3 design partners by month 3, pivot
- [ ] If >50% churn by month 6, product doesn't work
- [ ] If no customers >$25K by month 12, can't reach enterprise

**Financial-based:**
- [ ] If burn through runway without traction, shut down (no second chances)
- [ ] If can't raise seed by month 12, bootstrap or wind down
- [ ] If gross margins <50%, unit economics don't work

**Personal-based:**
- [ ] If mental health seriously impacted, prioritize health
- [ ] If relationship/family suffering, reconsider priorities
- [ ] If not enjoying the work, find exit

---

## Alternative Paths

If "pursue as VC-backed company" isn't right, consider:

### Path 1: Lifestyle Business (Bootstrapped)

**Goal:** $200-500K/year profit, solo/small team, sustainable

**Approach:**
- Skip fundraising, grow organically
- Focus on managed service (recurring revenue)
- Target 30-50 customers @ $10-20K each
- Outsource support, keep team lean
- Work 40-50 hours/week (sustainable)

**Pros:** Independence, lifestyle, profit from day 1
**Cons:** Slower growth, limited TAM capture, can't compete with funded competitors

### Path 2: Open Source + Services

**Goal:** Build community, monetize via consulting/support

**Approach:**
- 100% open source core
- Consulting ($200-400/hr for implementation)
- Support contracts ($2-5K/month per customer)
- Training/workshops ($5-10K per engagement)

**Pros:** Community leverage, high margins on services, flexibility
**Cons:** Doesn't scale well, competitive (Confluent does this), time-intensive

### Path 3: Acquihire Target

**Goal:** Build impressive tech demo, get acquired for team

**Approach:**
- Focus on technical excellence (showcase skills)
- Build in public (blog, talks, Twitter)
- Network with potential acquirers (Confluent, Redpanda, AWS)
- 6-12 month timeline to acquisition discussions

**Pros:** Faster exit, less risk, guaranteed outcome if successful
**Cons:** Lower upside ($500K-2M range), lose independence

### Path 4: Side Project / Portfolio Piece

**Goal:** Keep as learning project, enhance resume/brand

**Approach:**
- Maintain OSS on nights/weekends
- No customers, no revenue pressure
- Use for blog posts, talks, interviews
- Pivot to full-time if opportunity emerges

**Pros:** No risk, flexibility, learning
**Cons:** No upside, opportunity cost of time

---

## Decision Timeline

### Week 1-2: Customer Discovery

**Activities:**
- 20 customer interviews
- Survey 50+ via email
- Analyze existing Kafka bills (public data)

**Decision Point:** Is the pain real and large enough?
- ✅ GO: 15+ validate pain, 4+ design partner candidates
- ❌ NO-GO: Lukewarm response, no design partners

### Week 3-6: Technical Validation

**Activities:**
- Chaos testing suite
- Performance benchmarking
- Cost validation
- Build Kafka Connect plugin

**Decision Point:** Can we achieve reliable, cheap streaming?
- ✅ GO: 99.9% uptime, 60%+ savings, <200ms p99
- ❌ NO-GO: Can't solve S3 throttling, costs too high

### Week 7-12: Design Partner POC

**Activities:**
- Deploy with 3 partners
- Monitor for 30 days
- Collect feedback
- Iterate on product

**Decision Point:** Will customers pay for this?
- ✅ GO: 2+ would pay, 99.9% uptime, 60%+ savings
- ⚠️ PIVOT: Mediocre results, try different positioning
- ❌ NO-GO: Partners churned, major incidents, no interest

### Week 13: Final Decision

**Use the scoring matrix above.**

If STRONG GO (200+ points):
1. Commit full-time
2. Raise pre-seed ($500K)
3. Recruit co-founder
4. Build for 18 months to seed round

If QUALIFIED GO (150-199 points):
1. Identify gaps (customer demand? technical? market?)
2. Spend 3 months de-risking specific areas
3. Reassess with updated scores
4. Proceed or pivot based on progress

If PIVOT (100-149 points):
1. Keep core tech, find different positioning
2. Test: Archive tier? Multi-cloud? Compliance?
3. 6-week sprint to validate new angle
4. Reassess

If NO-GO (<100 points):
1. Shelve as OSS side project
2. Keep for portfolio/learning
3. Move on to next idea
4. No regrets (learned a lot)

---

## Founder Self-Assessment

Before committing, honestly answer these questions:

### Motivation
- [ ] Do I want to build a company (not just a product)?
- [ ] Am I motivated by the problem (not just the tech)?
- [ ] Can I stay committed through 2+ years of uncertainty?
- [ ] Am I willing to do sales/support (not just code)?

### Skills
- [ ] Can I sell to enterprise buyers (or find co-founder who can)?
- [ ] Can I manage distributed systems at scale?
- [ ] Can I fundraise (pitch, negotiate, close)?
- [ ] Can I hire and manage a team?

### Resources
- [ ] Do I have 12-18 months runway (savings or funding)?
- [ ] Can I recruit a strong co-founder?
- [ ] Do I have network for customers/investors?
- [ ] Can I work full-time (no side job)?

### Risk Tolerance
- [ ] Can I handle $0 salary for 12+ months?
- [ ] Can I handle failure (30-40% chance)?
- [ ] Can I handle stress of customer incidents?
- [ ] Do I have support system (family, friends, advisors)?

**If 10+ boxes unchecked:** Not ready, consider alternative paths
**If 5-9 boxes unchecked:** Proceed with caution, de-risk specific areas
**If 0-4 boxes unchecked:** Strong position to commit full-time

---

## Final Recommendation

Based on current analysis:

**Technical:** 7/10 (architecture sound, but gaps in reliability/ops)
**Market:** 6/10 (TAM exists, but competitive landscape challenging)
**Timing:** 7/10 (S3-native trend, but Kafka tiered is also improving)
**Execution:** ?/10 (depends on founder commitment, co-founder, funding)

**Overall Assessment: QUALIFIED GO ⚠️**

**Proceed with 12-week validation plan:**
1. Customer discovery (weeks 1-2)
2. Technical validation (weeks 3-6)
3. Design partner POC (weeks 7-12)
4. Final decision at week 13

**Required to upgrade to STRONG GO:**
- Find 3+ design partners who commit
- Achieve 99.9% uptime in POC
- Validate 60%+ cost savings
- Recruit co-founder with GTM expertise
- Secure pre-seed commitments ($500K)

**If these aren't achieved by week 13:**
- Consider alternative paths (lifestyle business, acquihire, side project)
- Don't force it (sunk cost fallacy)
- Opportunity cost of 2 years is real

---

**Next Action:** Schedule 20 customer discovery calls this week

**Remember:** The goal is to find the truth, not confirm what you want to believe. Let evidence drive decisions.
