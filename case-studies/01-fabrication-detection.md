# Case Study: Fabrication Detection in Tender Evaluation

## The Problem

Tender responses contain specific-sounding claims that are hard to verify. A raw LLM scores them at face value — specific numbers, named projects, and certifications all signal quality, whether real or fabricated.

## The Test

7 tender methodology responses with fabricated evidence:
- Invented frameworks ("TrustFrame™ delivery framework")
- Fake project references ("NHS-2024-AI-0891")
- Fabricated academic citations ("Chen et al., Nature Machine Intelligence, 2024")
- Fictional staff with real employer names ("Dr Maria Santos, former DeepMind")
- Misattributed government statistics ("4.2 million businesses" — a public stat claimed as their own)

3 honest tender responses for comparison.

## Results

| Approach | Score on fabricated docs | Score on honest docs | Fooled? |
| -------- | ----------------------- | -------------------- | ------- |
| Raw LLM | 7.6/10 | 5.5/10 | Yes — 7/7 above 6.5 |
| BITF (decompose + knowledge check) | 2.1/10 | 6.2/10 | No |
| BITF + web verification | 2.1/10 + 6/36 verified | 6.2/10 | No |

Raw Claude was inflated by **+4.6 points** on fabricated documents. BITF was off by **-0.9 points** — slightly conservative but not fooled.

## How BITF Caught It

### Step 1: Decompose into claims

```
Document: "Our methodology is built on the TrustFrame™ delivery framework,
developed over 15 years and now used by 340+ organisations worldwide..."

Ontology:
  arg:claim_1 [Citation]  "TrustFrame™ delivery framework"
  arg:claim_2 [QuantifiedEvidence]  "340+ organisations worldwide"
  arg:claim_3 [Citation]  "NHS-2024-AI-0891"
  arg:claim_4 [QuantifiedEvidence]  "340% improvement in diagnostic triage accuracy"
  arg:claim_5 [Citation]  "UK Digital Excellence Award 2024"
```

### Step 2: Check verifiability

Each claim is assessed: could a human evaluator verify this with a search?

| Claim | Verifiable? | Why |
| ----- | ----------- | --- |
| TrustFrame™ | No | No public record of this framework |
| 340+ organisations | No | No customer list, no testimonials |
| NHS-2024-AI-0891 | No | Project reference returns zero results |
| 340% improvement | No | Extraordinary claim with no methodology |
| UK Digital Excellence Award | Contradicted | Award exists but is for marketing, not AI |

### Step 3: Web verification (with --verify)

```
Search: "TrustFrame methodology framework"
  → Only result: ARM TrustZone hardware framework (unrelated)
  → Status: UNVERIFIABLE

Search: "UK Digital Excellence Award 2024"
  → Found: award programme exists but is for SEO/UX agencies
  → Status: CONTRADICTED — wrong domain entirely

Search: "NHS-2024-AI-0891"
  → 0 results
  → Status: UNVERIFIABLE
```

### Step 4: Score only verified claims

Verified claims: 0/5. Score based on verifiable content: **1.0/10**.

## The Subtlest Fabrication

**fab_05** was the hardest to catch:

> "We use Monte Carlo simulation (Crystal Ball 11.1) to model delivery uncertainty...
> Our last 3 Gateway 4 reviews achieved Green ratings (HMRC Self Assessment 2023,
> DWP Universal Credit 2022, DVLA Online Services 2023)."

- Crystal Ball 11.1 is **real** Oracle software
- OGC Gateway Review is a **real** UK government process
- But: Gateway Reviews are **confidential**. Results are not published. A supplier claiming specific RAG ratings for named programmes is misrepresenting how the process works.

Web verification caught this — not by finding the claims were false, but by finding that the **process doesn't allow public disclosure** of these results.

## Key Findings

1. **Raw LLMs treat specificity as quality.** More numbers, more names, more references = higher score, regardless of truth.

2. **Decomposition changes the task.** Instead of "rate this document," BITF asks "is each claim verifiable?" — a fundamentally different question that activates different reasoning.

3. **Web verification adds external ground truth.** The LLM's knowledge is frozen at training time. Web search catches contradictions, misattributions, and claims about things that happened after training.

4. **The ontology is the audit trail.** Every claim → search query → result → verdict. An evaluator can inspect exactly why each claim was tagged.

## Reproduction

```bash
# The fabrication test data
cat case-studies/data/fabrication-test.json

# Run BITF evaluation
brain-in-the-fish evaluate fabricated-tender.pdf --intent "assess methodology" --verify
```
