# StreamHouse: Business Assessment Package

**Date:** January 29, 2026
**Version:** 1.0
**Status:** Pre-seed evaluation phase

---

## Overview

This package contains a comprehensive business viability analysis for StreamHouse - an S3-native event streaming platform designed as a cost-optimized alternative to Apache Kafka.

**TL;DR:**
- ✅ **Technical:** Architecture is sound, prototype works
- ⚠️ **Market:** Niche exists but competitive (Kafka tiered storage)
- ⚠️ **Execution:** Requires 12-week validation before full commitment
- 💰 **Opportunity:** $1.5-2B TAM, potential $10M+ outcome if successful

---

## Document Index

### 1. [Executive Summary](./EXECUTIVE_SUMMARY.md) (15 min read)
**Purpose:** Quick overview for investors, advisors, potential co-founders

**Contents:**
- One-page pitch
- Key metrics dashboard
- Cost comparison charts
- Competitive positioning
- Financial model
- Funding ask
- Team needs

**Read this if:** You have 15 minutes and want the highlights

---

### 2. [Business Analysis](./BUSINESS_ANALYSIS.md) (60 min read)
**Purpose:** Comprehensive deep-dive into all aspects of commercialization

**Contents:**
- Market positioning (who, what, why)
- Performance & latency analysis (honest assessment)
- Correctness & trust (technical gaps)
- Competitive landscape (Kafka tiered, MSK, etc.)
- Operations & deployment (production readiness)
- Business model (pricing, sales motion)
- Path to company (18-month milestones)
- Risk assessment (what could go wrong)

**Read this if:** You're doing serious due diligence or planning to commit full-time

**Key Sections:**
- **Section 1:** Answers "whose bill are we reducing?" (storage + compute disaggregation)
- **Section 2:** Honest latency analysis (5-30x slower than Kafka)
- **Section 3:** Trust & correctness gaps (no exactly-once yet)
- **Section 4:** Competitive reality (Kafka tiered is strong)
- **Section 7:** What success looks like in 18 months

---

### 3. [Decision Framework](./DECISION_FRAMEWORK.md) (30 min read)
**Purpose:** Structured approach to decide: "Is this a company or a side project?"

**Contents:**
- Decision tree (should I pursue this?)
- Validation checklist (3-phase, 12-week plan)
- Go/No-Go scoring matrix
- Scenario planning (best/base/worst case)
- Commitment framework (what it really means)
- Alternative paths (bootstrap, acquihire, OSS)

**Read this if:** You're deciding whether to go full-time on this

**Use this to:**
- Score your validation results (0-260 point system)
- Set exit criteria (when to stop)
- Plan 12-week validation roadmap
- Understand alternative paths if VC-backed startup isn't right

---

## Quick Reference

### The Core Questions Answered

**1. Whose bill are we reducing?**
- Storage + compute costs for companies with 1-10TB data, 30+ day retention
- Target savings: 60-70% ($1-3K/month per customer)

**2. What workloads do we support?**
- ✅ Analytics, archival, ML training data, audit logs, compliance
- ❌ Low-latency (<10ms), exactly-once transactions, complex stream processing

**3. How fast are we?**
- Produce p99: 10ms (vs Kafka <1ms) - **10x slower**
- Consume p99 (hot): 10ms (vs Kafka <5ms) - **2x slower**
- Consume p99 (cold): 150ms (vs Kafka <5ms) - **30x slower**

**4. Why would someone choose us over Kafka tiered storage?**
- Multi-cloud (Kafka tiered is AWS-only today)
- Simpler architecture (no Kafka cluster at all)
- Lower cold-read cost (direct S3, no broker)
- Better for "archive tier" coexistence

**5. What are the biggest risks?**
- Migration risk > cost savings (customers don't switch)
- S3 throttling causes cascading failures
- Kafka tiered storage matures and closes gap
- Can't prove reliability at scale

**6. What's the first product (v1)?**
- "Kafka Archive Tier" - Connector that copies Kafka → StreamHouse
- Read-only, batch analytics, coexists with Kafka
- Non-critical workload, low risk, foot in door

**7. What needs to happen in next 12 weeks?**
- 20 customer interviews (validate pain)
- Chaos testing (prove reliability)
- 3 design partner POCs (30-day pilots)
- Decision: GO / PIVOT / NO-GO

**8. What's success in 18 months?**
- $500K ARR (60 customers @ $8-12K each)
- 99.9% uptime track record
- 3 reference customers saving 60%+
- Seed funded ($2-3M)

---

## Financial Snapshot

### Unit Economics (Mature Customer)

```
Customer: 5TB/month, 30-day retention

Revenue:        $1,200/month ($14.4K/year)
COGS:           $330/month
Gross Margin:   73%

CAC:            $1,500
Payback:        1.7 months
LTV (24mo):     $20,880
LTV/CAC:        14:1 ✅
```

### 18-Month Revenue Projection

| Month | Customers | MRR | ARR |
|-------|-----------|-----|-----|
| 3 | 3 | $1.5K | $18K |
| 6 | 10 | $6.7K | $80K |
| 12 | 30 | $25K | $300K |
| 18 | 60 | $60K | $720K |

### Funding Requirements

**Pre-seed:** $500K
- Use: 2 engineers, 1 GTM hire, AWS infra
- Runway: 12 months to $100K ARR
- Valuation: $5M post-money

**Seed:** $2-3M (month 12)
- Use: Hire to 7 people, build managed service
- Runway: 18 months to Series A
- Valuation: $15-20M post-money

---

## Competitive Positioning

### Where We Win

✅ **Long retention** (90+ days where storage >> compute)
✅ **Multi-cloud** (AWS + GCP + Azure, anywhere S3 works)
✅ **Archive tier** (coexist with Kafka, no migration)
✅ **Cost-sensitive** (customers prioritize savings over features)
✅ **Analytics workloads** (20-50ms latency acceptable)

### Where We Lose

❌ **Low latency required** (<10ms p99)
❌ **Exactly-once needed** (financial, transactional)
❌ **Mature ecosystem required** (Confluent support, tooling)
❌ **Migration risk averse** (don't want to touch working Kafka)
❌ **Feature parity expected** (need Kafka compatibility)

---

## Critical Path (Next 12 Weeks)

### Week 1-2: Customer Discovery
- [ ] Interview 20 companies (8 high Kafka bills, 6 new pipelines, 6 compliance)
- [ ] Validate pain exists and is severe (6+/10)
- [ ] Identify 4+ design partner candidates
- [ ] **Decision:** Is pain real? (GO/NO-GO)

### Week 3-6: Technical Validation
- [ ] Chaos testing (S3 throttling, failures, partitions)
- [ ] Performance benchmarks (10TB, 100M msgs/day)
- [ ] Cost validation (actual AWS bill <$600 for 10TB)
- [ ] Build Kafka Connect source plugin
- [ ] **Decision:** Can we prove reliability? (GO/NO-GO)

### Week 7-12: Design Partner POC
- [ ] Deploy with 3 partners
- [ ] Run for 30 days (measure uptime, cost, feedback)
- [ ] Iterate based on feedback
- [ ] **Decision:** Will customers pay? (GO/PIVOT/NO-GO)

### Week 13: Final Decision
- [ ] Score using decision matrix (0-260 points)
- [ ] 200+: STRONG GO (commit full-time, raise pre-seed)
- [ ] 150-199: QUALIFIED GO (3-month focused improvement)
- [ ] 100-149: PIVOT (try different positioning)
- [ ] <100: NO-GO (shelve or move on)

---

## Team Requirements

### Immediate (Month 0-3)
- **Technical Founder** (You)
  - Full-time commitment
  - Product + Engineering + Architecture

- **Co-Founder** (To recruit)
  - GTM expertise (sold to enterprise infrastructure before)
  - Equity: 30-40%
  - Commitment: Full-time from day 1

### First Hires (Month 6+)
- **Solutions Engineer / SDR** (Month 6)
  - Outbound, demos, POCs
  - $80-120K + equity

- **Backend Engineer** (Month 9)
  - Distributed systems experience
  - $140-180K + equity

- **Support Engineer** (Month 12)
  - Customer success, on-call
  - $100-140K + equity

---

## Key Metrics to Track

### Product Metrics
- **Uptime:** Target 99.9% (measure with public status page)
- **Latency:** p50/p95/p99 for produce and consume
- **Cost savings:** Customer Kafka bill vs StreamHouse bill
- **Scale:** Max messages/day, max data stored

### Business Metrics
- **MRR:** Monthly recurring revenue
- **ARR:** Annual recurring revenue
- **CAC:** Customer acquisition cost
- **LTV:** Lifetime value (track churn)
- **Gross margin:** Revenue - COGS

### Customer Metrics
- **NPS:** Net promoter score (would recommend?)
- **Churn:** % customers leaving per month
- **Expansion:** % customers increasing usage
- **Support load:** Hours/week per customer

---

## Red Flags & Exit Criteria

### Stop If (Week 4)
- ❌ Can't find 3 design partners after 20 interviews
- ❌ Customers say "interesting but wouldn't use"
- ❌ No clear pain point (Kafka bill isn't that bad)

### Stop If (Week 8)
- ❌ Can't solve S3 throttling issue
- ❌ Can't achieve 99.9% uptime in testing
- ❌ Cost savings <50% in practice

### Stop If (Week 12)
- ❌ Design partners churn or won't pay
- ❌ Major reliability incidents (data loss)
- ❌ Scoring matrix <100 points

### Stop If (Month 12)
- ❌ <$100K ARR
- ❌ >60% annual churn
- ❌ Can't raise seed funding
- ❌ Gross margins <50%

### Stop If (Month 18)
- ❌ <$500K ARR
- ❌ Flat/declining growth
- ❌ Multiple P0 incidents
- ❌ No path to profitability

**Important:** Set these criteria upfront to avoid sunk cost fallacy

---

## Alternative Outcomes

If "VC-backed startup" doesn't work out:

### 1. Lifestyle Business (Bootstrapped)
- Target: $200-500K/year profit
- Customers: 30-50 @ $10-20K each
- Team: Solo or 2-3 people
- Hours: 40-50/week (sustainable)

### 2. Open Source + Consulting
- Model: Free software, paid services
- Revenue: Consulting, support contracts, training
- Scale: Limited but high margin

### 3. Acquihire
- Goal: Get acquired for team ($500K-2M)
- Timeline: 6-12 months of impressive building
- Buyers: Confluent, Redpanda, AWS, Databricks

### 4. Side Project
- Keep as portfolio piece
- No revenue pressure
- Use for learning, blogging, networking

---

## Recommended Reading Order

**If you have 15 minutes:**
1. This README
2. [Executive Summary](./EXECUTIVE_SUMMARY.md)

**If you have 1 hour:**
1. [Executive Summary](./EXECUTIVE_SUMMARY.md)
2. [Decision Framework](./DECISION_FRAMEWORK.md) - Sections 1-3
3. [Business Analysis](./BUSINESS_ANALYSIS.md) - Sections 1, 7, 9

**If you're doing deep diligence:**
1. [Decision Framework](./DECISION_FRAMEWORK.md) - Complete
2. [Business Analysis](./BUSINESS_ANALYSIS.md) - Complete
3. [Executive Summary](./EXECUTIVE_SUMMARY.md) - For quick reference

**If you're a potential investor:**
1. [Executive Summary](./EXECUTIVE_SUMMARY.md)
2. [Business Analysis](./BUSINESS_ANALYSIS.md) - Sections 6-7
3. [Decision Framework](./DECISION_FRAMEWORK.md) - Section 3 (Go/No-Go)

**If you're a potential co-founder:**
1. [Decision Framework](./DECISION_FRAMEWORK.md) - Complete (understand commitment)
2. [Business Analysis](./BUSINESS_ANALYSIS.md) - Sections 4-6 (market, ops, business)
3. Technical README (understand the product)

---

## Next Actions

### This Week
- [ ] Read all three documents completely
- [ ] Reflect on commitment level (Decision Framework self-assessment)
- [ ] Create list of 50 potential customer interview targets
- [ ] Draft interview script (see Business Analysis Section 1)
- [ ] Set up tracking spreadsheet (interviews, metrics, milestones)

### Next Week
- [ ] Schedule 10 customer interviews
- [ ] Begin chaos testing setup
- [ ] Draft Kafka Connect plugin spec
- [ ] Identify co-founder candidates (3-5 people to reach out to)

### Week 3-4
- [ ] Complete 20 customer interviews
- [ ] Analyze results (scoring matrix in Decision Framework)
- [ ] Week 4 checkpoint: GO/NO-GO on technical validation phase

---

## Questions & Discussion

For questions or feedback on this analysis:

**Technical:** [Your email]
**Business:** [Your email]
**GitHub Issues:** [Repo link]

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-29 | Initial comprehensive analysis |

---

## Appendix: Key Terms

**TAM:** Total Addressable Market (max revenue if you captured 100% of target market)
**ARR:** Annual Recurring Revenue (predictable yearly revenue from subscriptions)
**MRR:** Monthly Recurring Revenue (ARR / 12)
**CAC:** Customer Acquisition Cost (sales + marketing cost per new customer)
**LTV:** Lifetime Value (total revenue from a customer over their lifetime)
**Churn:** % of customers who cancel per time period
**NPS:** Net Promoter Score (customer satisfaction metric)
**POC:** Proof of Concept (pilot deployment to test product)
**GTM:** Go-To-Market (strategy for selling and distributing product)

---

**Remember:** The goal is to find the truth, not confirm what you want to believe. Let evidence drive decisions.

**Good luck! 🚀**
