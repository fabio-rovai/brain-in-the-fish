# The Brain in the Fish — Design Document

## Vision

MiroFish gave fish a swarm. AgentSociety gave agents a mind. The Brain in the Fish gives the mind a skeleton — a structured, queryable, diffable, reasoned-over ontology substrate that agents don't just *use* but *exist within*.

**One-liner:** A Rust-native universal document evaluation engine where a swarm of cognitively-modelled AI agents evaluate ANY document for ANY purpose — marking, scoring, auditing, reviewing — with their entire mental state living inside an OWL ontology powered by open-ontologies.

**Key principle:** The system doesn't know what it's evaluating or why until you tell it. You provide a document + an intent ("mark this essay", "score this bid", "audit this policy"). The three ontologies adapt to any domain.

---

## Universal Evaluation — The Three Ontologies Adapt

| Use Case | Document Ontology | Criteria Ontology | Agent Ontology |
|---|---|---|---|
| **Mark a student essay** | Paragraphs, arguments, citations, thesis | Marking rubric, grade boundaries, learning outcomes | Academic markers with subject expertise |
| **Score a tender bid** | Bid sections, claims, evidence, case studies | ITT criteria, weights, pass/fail thresholds | Procurement panel with domain specialisms |
| **Assess a policy** | Objectives, measures, impact projections | Policy framework, impact criteria, stakeholder needs | Policy analysts, public reps, domain experts |
| **Analyse survey results** | Response themes, sentiment clusters, demographics | Research questions, validity criteria, statistical thresholds | Methodologists, domain experts, statisticians |
| **Review a contract** | Clauses, obligations, terms, definitions | Legal checklist, risk criteria, regulatory requirements | Legal reviewers, compliance officers |
| **Audit a report** | Findings, data tables, conclusions, methodology | Reporting standards, completeness, accuracy | Auditors with different specialisms |

The user provides two inputs:
1. **The document** (PDF, DOCX, text)
2. **The intent** (natural language: "Mark this essay against the AQA A-level rubric" or "Score this bid against the attached ITT")

The system generates appropriate ontologies for both.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    brain-in-the-fish                      │
│                    Single Rust Binary                     │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  Document    │  │   Criteria   │  │    Agent      │  │
│  │  Ontology    │  │  Ontology    │  │   Ontology    │  │
│  │              │  │              │  │               │  │
│  │ Adapts to    │  │  Adapts to   │  │  Adapts to    │  │
│  │ document     │  │  evaluation  │  │  domain       │  │
│  │ type: essay, │  │  purpose:    │  │  expertise:   │  │
│  │ bid, policy, │  │  marking,    │  │  markers,     │  │
│  │ survey,      │  │  scoring,    │  │  analysts,    │  │
│  │ contract...  │  │  auditing... │  │  reviewers... │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬────────┘  │
│         │                 │                  │           │
│         └────────┬────────┴──────────┬───────┘           │
│                  ▼                   ▼                    │
│         ┌────────────────┐  ┌────────────────┐           │
│         │   onto_align   │  │  onto_reason   │           │
│         │ Maps bid ←→    │  │ Infers across  │           │
│         │ criteria ←→    │  │ all three      │           │
│         │ agent beliefs  │  │ ontologies     │           │
│         └────────────────┘  └────────────────┘           │
│                  │                   │                    │
│                  ▼                   ▼                    │
│         ┌─────────────────────────────────┐              │
│         │       Oxigraph Triple Store     │              │
│         │    (shared Arc<GraphStore>)      │              │
│         └─────────────────────────────────┘              │
│                          │                               │
│              ┌───────────┴───────────┐                   │
│              ▼                       ▼                   │
│    ┌──────────────────┐   ┌──────────────────┐          │
│    │   MCP Server     │   │   Tauri Studio   │          │
│    │  (stdio + HTTP)  │   │   (Vue 3 + D3)   │          │
│    └──────────────────┘   └──────────────────┘          │
│                                                          │
├─────────────────────────────────────────────────────────┤
│  Agents: Claude Agent SDK subagents (each agent is a    │
│  real Claude instance with full context + MCP tools)     │
└─────────────────────────────────────────────────────────┘
```

---

## The Three Ontologies

### 1. Document Ontology — What's being evaluated

Generated automatically from the uploaded document. Adapts its class hierarchy to the document type.

**Example A: Marking a student essay**
```turtle
@prefix doc: <http://brain-in-the-fish.dev/doc/> .
@prefix eval: <http://brain-in-the-fish.dev/eval/> .

doc:Document a eval:StudentEssay ;
    eval:title "The Impact of Quantitative Easing on UK Inflation" ;
    eval:totalWordCount 2500 ;
    eval:module "ECN301 Macroeconomics" .

doc:Introduction a eval:Section ;
    eval:title "Introduction" ;
    eval:wordCount 320 ;
    eval:containsClaim doc:Claim_QE_Inflation ;
    eval:statesThesis doc:Thesis_Main .

doc:Thesis_Main a eval:Thesis ;
    eval:text "QE had a limited direct impact on CPI inflation but significantly inflated asset prices" ;
    eval:specificity 0.8 ;
    eval:arguable true .

doc:Para3 a eval:Paragraph ;
    eval:parentSection doc:LitReview ;
    eval:citesSources 3 ;
    eval:containsArgument doc:Arg_MonetaryBase ;
    eval:containsEvidence doc:Evidence_BOE_Data .

doc:Evidence_BOE_Data a eval:Evidence ;
    eval:source "Bank of England Quarterly Bulletin, 2023" ;
    eval:type "Primary data" ;
    eval:recentEnough true .
```

**Example B: Scoring a tender bid**
```turtle
doc:Document a eval:TenderBid ;
    eval:title "Acme Corp Response to ITT-2026-0042" ;
    eval:totalPages 47 .

doc:Section3 a eval:Section ;
    eval:title "Technical Approach" ;
    eval:containsClaim doc:Claim_AI_10yr ;
    eval:citesEvidence doc:CaseStudy_NHS .

doc:CaseStudy_NHS a eval:CaseStudy ;
    eval:client "NHS Digital" ;
    eval:outcome "£2.1M cost savings" ;
    eval:hasQuantifiedOutcome true .
```

**Example C: Auditing survey methodology**
```turtle
doc:Document a eval:SurveyReport ;
    eval:title "Public Attitudes to AI in Healthcare" ;
    eval:sampleSize 2400 ;
    eval:methodology "Stratified random sampling" .

doc:Section_Method a eval:Section ;
    eval:title "Methodology" ;
    eval:describesMethod doc:Method_Sampling ;
    eval:containsClaim doc:Claim_Representative .

doc:Claim_Representative a eval:Claim ;
    eval:text "Sample is representative of UK adult population" ;
    eval:supportedBy doc:Evidence_ONS_Comparison ;
    eval:verifiable true .
```

The LLM generates the appropriate class hierarchy based on the user's intent. The base vocabulary (`eval:Section`, `eval:Claim`, `eval:Evidence`) is shared; domain-specific classes (`eval:Thesis`, `eval:CaseStudy`, `eval:Method`) are generated per use case.

### 2. Criteria Ontology — How to evaluate it

Generated from user-provided rubric/spec, or loaded from built-in frameworks. Adapts to evaluation purpose.

#### Essay marking example

```turtle
@prefix crit: <http://brain-in-the-fish.dev/criteria/> .
@prefix eval: <http://brain-in-the-fish.dev/eval/> .

crit:Framework a eval:EvaluationFramework ;
    eval:name "AQA A-Level Economics Marking Scheme" ;
    eval:totalWeight 1.0 ;
    eval:passMark 40 .

crit:Knowledge a eval:EvaluationCriterion ;
    eval:title "Knowledge and understanding" ;
    eval:maxScore 8 ;
    eval:weight 0.25 ;
    eval:parentCriterion crit:AO1 .

crit:Knowledge_L4 a eval:RubricLevel ;
    eval:criterion crit:Knowledge ;
    eval:level "Level 4" ;
    eval:scoreRange "7-8" ;
    eval:descriptor "Accurate and thorough knowledge with precise use of terminology" .

crit:Knowledge_L1 a eval:RubricLevel ;
    eval:criterion crit:Knowledge ;
    eval:level "Level 1" ;
    eval:scoreRange "1-2" ;
    eval:descriptor "Limited knowledge with significant gaps or inaccuracies" .
```

#### Tender scoring example

```turtle
crit:Framework a eval:EvaluationFramework ;
    eval:name "ITT-2026-0042 Quality Criteria" ;
    eval:totalWeight 1.0 ;
    eval:passMark 60 .

crit:C3_2 a eval:EvaluationCriterion ;
    eval:title "Demonstrate relevant experience" ;
    eval:maxScore 10 ;
    eval:weight 0.15 ;
    eval:requiresEvidence true ;
    eval:minimumCaseStudies 2 .
```

#### Policy audit example

```turtle
crit:Framework a eval:EvaluationFramework ;
    eval:name "Green Book Impact Assessment" ;
    eval:totalWeight 1.0 .

crit:EvidenceBase a eval:EvaluationCriterion ;
    eval:title "Quality of evidence base" ;
    eval:maxScore 5 ;
    eval:weight 0.20 ;
    eval:descriptor "Are claims supported by cited, recent, peer-reviewed evidence?" .

crit:StakeholderVoice a eval:EvaluationCriterion ;
    eval:title "Stakeholder representation" ;
    eval:maxScore 5 ;
    eval:weight 0.15 ;
    eval:descriptor "Are affected communities represented in the evidence?" .
```

The system ships with built-in frameworks (via `onto_marketplace`) for common evaluation types. Users can also provide their own rubric as a document or inline text.

### 3. Agent Ontology — Who's evaluating (AgentSociety cognitive model)

Each evaluator agent's entire mental state is modelled as OWL.

```turtle
@prefix agent: <http://brain-in-the-fish.dev/agent/> .
@prefix cog: <http://brain-in-the-fish.dev/cognition/> .
@prefix eval: <http://brain-in-the-fish.dev/eval/> .

# --- Agent Identity ---
agent:Margaret a eval:EvaluatorAgent ;
    eval:name "Margaret Chen" ;
    eval:role "Procurement Lead" ;
    eval:yearsExperience 20 ;
    eval:domain "Public Sector Procurement" .

# --- Maslow Needs (evaluation context) ---
agent:Margaret cog:hasNeed agent:Margaret_Need_Safety .
agent:Margaret_Need_Safety a cog:SafetyNeed ;
    cog:expression "Compliance and risk mitigation" ;
    cog:salience 0.9 ;   # very important to this agent
    cog:satisfied false . # not yet assessed

agent:Margaret cog:hasNeed agent:Margaret_Need_Esteem .
agent:Margaret_Need_Esteem a cog:EsteemNeed ;
    cog:expression "Recognition of thorough evaluation" ;
    cog:salience 0.6 .

# --- Theory of Planned Behaviour ---
agent:Margaret cog:hasAttitude agent:Margaret_Attitude_C3_2 .
agent:Margaret_Attitude_C3_2 a cog:Attitude ;
    cog:toward crit:C3_2 ;
    cog:valence 0.0 ;      # neutral before reading
    cog:confidence 0.0 .   # no opinion yet

agent:Margaret cog:subjectiveNorm agent:Margaret_Norm_C3_2 .
agent:Margaret_Norm_C3_2 a cog:SubjectiveNorm ;
    cog:toward crit:C3_2 ;
    cog:perceivedGroupOpinion 0.0 ;  # unknown
    cog:influencedBy agent:James ;    # trusts James on technical matters
    cog:trustWeight 0.7 .

agent:Margaret cog:perceivedControl agent:Margaret_Control_C3_2 .
agent:Margaret_Control_C3_2 a cog:PerceivedControl ;
    cog:toward crit:C3_2 ;
    cog:confidence 0.8 .  # feels competent to evaluate this

# --- Working Memory ---
agent:Margaret cog:workingMemory agent:Margaret_WM .
agent:Margaret_WM a cog:WorkingMemory ;
    cog:currentFocus crit:C3_2 ;
    cog:evidenceSeen bid:CaseStudy_NHS_Digital ;
    cog:tentativeScore 6 ;
    cog:openQuestions "No data governance evidence found" .

# --- Social Relationships ---
agent:Margaret cog:trustsAgent agent:James ;
    cog:trustDomain "Technical assessment" ;
    cog:trustLevel 0.7 .

agent:Margaret cog:trustsAgent agent:Sarah ;
    cog:trustDomain "Social value" ;
    cog:trustLevel 0.5 .
```

---

## The Evaluation Pipeline

### Stage 1: Ingest (Document → Document Ontology)

```
Input:  document.pdf + intent ("mark this essay against AQA rubric")
        │
        ▼
   ┌─────────────┐
   │  PDF Parser  │  lopdf + pdftotext (pure Rust)
   │              │  Extract text, headings, tables
   └──────┬───────┘
          │
          ▼
   ┌──────────────────┐
   │  Section Splitter │  Detect heading hierarchy
   │  (Rust)           │  Split into logical sections
   └──────┬────────────┘
          │
          ▼
   ┌──────────────────┐
   │  Content Extractor│  LLM call: informed by intent
   │  (Claude API)     │  Essay → arguments, thesis, citations
   │                   │  Bid → claims, evidence, case studies
   │                   │  Survey → methodology, findings, stats
   │                   │  Policy → objectives, measures, impacts
   └──────┬────────────┘
          │
          ▼
   ┌──────────────────┐
   │  Turtle Generator │  Convert structured extraction
   │  (Rust)           │  into document ontology triples
   └──────┬────────────┘
          │
          ▼
   onto_load(turtle: "...")  →  Oxigraph store
   onto_embed()              →  Text + structural embeddings
```

The intent string drives the LLM's extraction strategy. Same pipeline, different prompt.

### Stage 2: Framework (Intent → Criteria Ontology)

Three modes:

**A) User provides rubric/spec as document:**
```
rubric.pdf → PDF Parser → LLM extracts criteria, weights, rubrics
           → Turtle Generator → onto_load
```

**B) User describes criteria in natural language:**
```
Intent: "Mark for argument quality, use of evidence, and writing clarity"
→ LLM generates criteria ontology from description
→ onto_load
```

**C) Built-in framework from marketplace:**
```
onto_marketplace(action: "install", id: "eval-essay-academic")
onto_marketplace(action: "install", id: "eval-tender-quality")
onto_marketplace(action: "install", id: "eval-policy-greenbook")
onto_marketplace(action: "install", id: "eval-survey-methodology")
onto_marketplace(action: "install", id: "eval-generic-quality")
```

Built-in frameworks ship with the binary. Users can also create and share custom frameworks.

### Stage 3: Alignment (Document ←→ Criteria mapping)

```
onto_align(source: "bid ontology", target: "criteria ontology")
→ Returns mapping matrix with confidence scores:

  Criterion 3.2 ←→ Section 4    (0.91)
  Criterion 3.2 ←→ Section 3.1  (0.73)
  Criterion 2.1 ←→ Section 3    (0.88)
  Criterion 5.1 ←→ [NO MATCH]   (0.00)  ← GAP DETECTED

onto_reason(profile: "owl-rl")
→ Infers: "Criterion X requires evidence. No document section maps to it.
   Therefore: eval:hasGap true."
```

### Stage 4: Agent Spawning

Agents are generated based on the **evaluation purpose**, not hardcoded.

```
Input: intent string + criteria ontology

LLM generates evaluator panel appropriate to the domain:

  Intent: "Mark this essay"
  → "Dr. Torres" (Subject Expert, depth of knowledge)
  → "Prof. Lin" (Academic Writing, structure and clarity)
  → "Dr. Osei" (Critical Thinking, argument quality)
  → "Moderator" (synthesises, challenges outliers)

  Intent: "Score this tender bid"
  → "Margaret Chen" (Procurement Lead, compliance)
  → "James Okafor" (Domain Expert, technical depth)
  → "Sarah Williams" (Social Value Champion)
  → "David Park" (Finance, value for money)
  → "Moderator"

  Intent: "Audit this survey methodology"
  → "Dr. Reeves" (Statistician, sampling validity)
  → "Dr. Nakamura" (Domain Expert, question design)
  → "Dr. Ahmed" (Ethics, participant representation)
  → "Moderator"

For each agent:
  1. Generate persona via LLM (name, background, biases, expertise)
  2. Generate cognitive state as Turtle (Maslow needs, attitudes, trust)
  3. onto_load(turtle: agent_triples)
  4. Assign criteria based on agent expertise:
     onto_search maps agent domain → relevant criteria

onto_embed()  # re-embed with agent triples included
```

### Stage 5: Independent Scoring (Round 1)

Each agent executes a ReACT loop. Example — marking an essay:

```
Agent: Dr. Torres (Subject Expert)
Criterion: Knowledge "Knowledge and understanding"

THOUGHT: I need to assess depth of subject knowledge.

ACTION: onto_search("economic theory quantitative easing", top_k=5)
RESULT: [doc:LitReview (0.93), doc:Analysis (0.81)]

ACTION: onto_similarity(crit:Knowledge, doc:LitReview)
RESULT: {cosine: 0.93, poincare: 0.12, product: 0.90}

ACTION: onto_query("SELECT ?src ?type WHERE {
    doc:LitReview eval:citesSources ?count .
    doc:LitReview eval:containsArgument ?arg .
    ?arg eval:supportedBy ?ev .
    ?ev eval:source ?src ; eval:type ?type .
}")
RESULT: [{src: "Bank of England, 2023", type: "Primary data"},
         {src: "Krugman 2015", type: "Academic"},
         {src: "ONS CPI series", type: "Statistical"}]

THOUGHT: Strong use of primary sources. Three distinct evidence types.
         Terminology is precise ("monetary base", "transmission mechanism").
         Minor gap: no mention of velocity of money.

SCORE: 7/8

→ Insert triples:
  agent:DrTorres eval:scores [
      eval:criterion crit:Knowledge ;
      eval:score 7 ;
      eval:round 1 ;
      eval:justification "Strong primary sources (BoE, ONS). Precise terminology.
                          Three evidence types cited. Minor gap: velocity of
                          money not addressed." ;
      eval:evidenceUsed doc:Evidence_BOE_Data, doc:Evidence_Krugman ;
      eval:gapIdentified "No discussion of velocity of money"
  ] .

→ Update cognitive state:
  agent:DrTorres_Attitude_Knowledge cog:valence 0.75 .
  agent:DrTorres_Attitude_Knowledge cog:confidence 0.85 .

→ onto_version("round_1")
```

### Stage 6: Debate (Rounds 2-N)

```
DEBATE PROTOCOL:

For each round:
  1. Each agent reads all other agents' scores via SPARQL:

     SELECT ?agent ?criterion ?score ?justification WHERE {
         ?agent eval:scores ?scoring .
         ?scoring eval:criterion ?criterion ;
                  eval:score ?score ;
                  eval:round ?currentRound ;
                  eval:justification ?justification .
     }

  2. Agent identifies disagreements (score delta > 2):

     Margaret scored C3_2 = 6
     James scored C3_2 = 8  ← delta = 2, challenge triggered

  3. Agent generates challenge via LLM (grounded in ontology):

     Margaret → James:
     "You scored 8 but onto_query shows only ONE case study with
      quantified outcomes. The rubric (crit:C3_2_Rubric_Excellent)
      requires 'three or more'. What evidence supports 8?"

  4. Challenged agent re-evaluates:

     James re-queries: onto_search("technical experience evidence")
     Finds additional evidence in Section 3.1 (team CVs with project history)
     Updates: score 8 → 7, justification updated

  5. Insert challenge + response as triples:

     agent:James eval:challengedBy [
         eval:challenger agent:Margaret ;
         eval:criterion crit:C3_2 ;
         eval:round 2 ;
         eval:argument "Rubric requires 3+ quantified case studies" ;
         eval:response "Acknowledged. Section 3.1 CVs provide partial
                       evidence but not formal case studies. Adjusting." ;
         eval:scoreChange "8 → 7"
     ] .

  6. Update trust weights:

     agent:James cog:trustsAgent agent:Margaret ;
         cog:trustLevel 0.8 .  # increased from 0.7 — good challenge

  7. onto_version("round_2")

CONVERGENCE CHECK (after each round):
  onto_drift("round_N-1", "round_N")
  → If drift_velocity < 0.1 (scores barely moving): CONVERGED
  → If round > 5 and not converged: STOP, flag dissent

  onto_monitor triggers:
  - "All scores within 2 points" → converged
  - "Agent X hasn't changed in 3 rounds" → entrenched, flag as dissent
```

### Stage 7: Moderation & Consensus

```
Moderator agent (Dr. Osei) runs after convergence:

1. onto_query: Get final scores per criterion per agent
2. Calculate: weighted mean, median, std deviation
3. Identify outliers: score > 2 std from mean
4. For each outlier:
   - Interview agent: "Your score of 4 is 2.3 std below panel mean of 7.1.
     The debate log shows Margaret challenged you on this.
     Do you maintain your score?"
   - If maintained: record as formal dissent
   - If adjusted: record adjustment with justification

5. Final consensus:
   - Moderated score = weighted mean (trust-weighted, not simple average)
   - Agent trust weights influence their contribution to final score
   - Dissenting scores recorded but not averaged in

6. Insert final scores:
   eval:FinalScore_C3_2 a eval:ModeratedScore ;
       eval:criterion crit:C3_2 ;
       eval:consensusScore 6.8 ;
       eval:panelMean 6.75 ;
       eval:panelStdDev 0.96 ;
       eval:dissent [
           eval:agent agent:Sarah ;
           eval:score 4 ;
           eval:reason "No social value integration in technical approach"
       ] .
```

### Stage 8: Report Generation

```
ReACT Report Agent generates:

1. EXECUTIVE SUMMARY
   - Overall score: 67/100
   - Pass/Fail: PASS (threshold: 60)
   - Top 3 strengths, Top 3 weaknesses
   - Risk flags

2. CRITERION SCORECARD
   ┌────────────┬───────┬────────┬─────────┬──────────┐
   │ Criterion  │ Score │ Max    │ Weight  │ Weighted │
   ├────────────┼───────┼────────┼─────────┼──────────┤
   │ C3.2 Exp.  │ 6.8   │ 10     │ 0.15    │ 1.02     │
   │ C2.1 Tech  │ 7.5   │ 10     │ 0.20    │ 1.50     │
   │ C5.1 SV    │ 3.2   │ 10     │ 0.10    │ 0.32     │
   └────────────┴───────┴────────┴─────────┴──────────┘

3. GAP ANALYSIS (from onto_align gaps)
   - Criterion 5.1: NO matching bid content
   - Criterion 4.3: Partial match (confidence 0.45)

4. DEBATE TRAIL (per criterion)
   - Round 1: Independent scores
   - Round 2: Challenges issued
   - Round 3: Convergence
   - Dissenting opinions preserved

5. IMPROVEMENT RECOMMENDATIONS
   - Per criterion: specific, actionable, referencing rubric levels
   - Essay: "To reach Level 4: address velocity of money, add one
     more primary source, strengthen conclusion's link to thesis"
   - Bid: "To move from 6.8 to 9+: add two more quantified case studies"
   - Policy: "Evidence base lacks stakeholder voice — add consultation data"

6. ONTOLOGY EXPORT
   - Full evaluation state as Turtle file
   - Importable into open-ontologies for cross-evaluation analysis
   - Enables cross-document comparison over time
```

### Stage 9: Deep Interaction

```
User can query any aspect via natural language or SPARQL:

NL: "Why did knowledge score 7 not 8?"
→ onto_search("knowledge understanding") → finds criterion
→ onto_query: get all agent scores + justifications
→ onto_query: get debate trail
→ Interview Dr. Torres: "What gap prevented full marks?"
→ Synthesise answer: "Velocity of money not addressed"

NL: "What if I added a paragraph about monetary velocity?"
→ User provides text
→ Generate new document triples
→ onto_align: map new content to criteria
→ Re-run affected agents (only those scoring Knowledge)
→ Show score delta: "7 → 8, gap resolved"

NL: "How does this essay compare to the last batch I marked?"
→ onto_search across historical evaluation ontologies
→ onto_align criteria frameworks (same rubric = trivial)
→ onto_diff score patterns
→ "This essay scores in the 75th percentile for evidence quality
    but 30th percentile for argument structure compared to
    the last 12 essays you evaluated"
```

---

## Tech Stack

| Component | Technology | Justification |
|-----------|-----------|---------------|
| **Core binary** | Rust (edition 2024) | Single binary, matches open-ontologies |
| **Triple store** | Oxigraph 0.4 (via open-ontologies) | In-process, no external DB |
| **Ontology ops** | open-ontologies as library crate | Direct `Arc<GraphStore>` — zero network overhead |
| **Embeddings** | tract-onnx + BGE-small | Same as open-ontologies, in-process |
| **PDF parsing** | lopdf + pdftotext (or pdf-extract-api) | Pure Rust PDF extraction |
| **LLM** | Claude Agent SDK (subagents) | Each evaluator agent is a real Claude subagent with full context + MCP tool access |
| **MCP transport** | rmcp 1.1.1 (stdio + HTTP) | Expose evaluation tools to Claude Code/Desktop |
| **Desktop UI** | Tauri 2 + Vue 3 + D3.js | Matches open-ontologies Studio pattern |
| **State/audit** | SQLite via rusqlite | Lineage, feedback, versioning |

### Relationship to open-ontologies

**Not a fork. A dependent crate.**

```toml
# brain-in-the-fish/Cargo.toml
[dependencies]
open-ontologies-core = { path = "../open-ontologies", features = ["embeddings"] }
```

brain-in-the-fish imports the core graph store, reasoner, aligner, and embedder. It adds:
- PDF ingestion pipeline
- Agent cognitive model (Maslow + TPB as OWL)
- Debate orchestrator
- Evaluation-specific MCP tools
- Report generator
- Tauri frontend adapted for evaluation workflow

---

## MCP Tools Exposed

brain-in-the-fish exposes its own MCP tools (prefixed `eval_*`) alongside inherited `onto_*` tools:

| Tool | Purpose |
|------|---------|
| `eval_ingest` | Upload document (PDF/DOCX/text) + intent, extract document ontology |
| `eval_criteria` | Load/parse/generate evaluation criteria from rubric, spec, or intent |
| `eval_align` | Map document sections to criteria |
| `eval_spawn` | Generate domain-appropriate evaluator agent panel |
| `eval_score` | Run independent scoring round |
| `eval_debate` | Run debate round |
| `eval_converge` | Check convergence, trigger moderation |
| `eval_report` | Generate evaluation report |
| `eval_chat` | Interactive query (NL or SPARQL) |
| `eval_compare` | Cross-evaluation analysis (compare documents over time) |
| `eval_replay` | Re-run from any versioned round |
| `eval_fork` | Fork evaluation with different criteria or agent panel |
| `eval_whatif` | "What if I changed this section?" — partial re-evaluation |
| All `onto_*` tools | Full open-ontologies capability |

---

## Frontend (Tauri Studio — Evaluation Mode)

### Step-based workflow (matching MiroFish's 5-step pattern):

| Step | View | Description |
|------|------|-------------|
| 1 | **Upload & Intent** | Drop document + describe what you want ("mark this essay", "audit this policy"). Watch document ontology build in real-time. D3 graph shows sections, claims, evidence as nodes |
| 2 | **Criteria & Alignment** | Load rubric/spec or select built-in framework. Sankey diagram shows document ←→ criteria mapping with confidence. Gaps highlighted red |
| 3 | **Agent Panel** | See spawned evaluator personas (adapted to domain). Edit if needed. Cognitive state visible as expandable OWL tree. Trust network as force-directed graph |
| 4 | **Debate** | Live debate feed. Score convergence chart updating in real-time. Click any agent to see their full cognitive state. Diff view between rounds |
| 5 | **Report & Interact** | Final scorecard + debate trail. Chat with any agent. "What if?" re-evaluation. Fork/replay controls. Export as PDF/Turtle |

---

## What Makes This Novel

1. **Evaluation over prediction** — MiroFish predicts futures (speculative, hallucination-prone). Brain in the Fish evaluates documents against criteria (concrete, verifiable). The LLM scores against structured evidence, not imagined outcomes. This is a fundamentally more reliable use of agent swarms.

2. **Ontology-native agent cognition** — Agent mental states are OWL, not JSON/Python dicts. Queryable, reasoned over, diffable. The agents don't just use the knowledge graph — they exist within it.

3. **Universal document evaluation** — Same engine evaluates essays, bids, policies, surveys, contracts, reports. The three ontologies (document, criteria, agent) adapt to any domain via intent-driven generation.

4. **Structured evidence mapping** — `onto_align` provides mathematical grounding for "does this section answer this question?" Not vibes. Agents argue from evidence maps, not prompt engineering.

5. **Versioned debate** — Every round is a snapshot. `onto_diff` between any two. `onto_rollback` to any point. Fork alternative evaluation paths. No other system can do this.

6. **Cross-evaluation intelligence** — After N evaluations, the system accumulates ontological knowledge about scoring patterns, criteria equivalence, common gaps. Gets smarter with use.

7. **Single Rust binary** — No Python, no Node, no cloud dependencies. `brain-in-the-fish serve` and go.

8. **Open-ontologies native** — Not bolted on. The triple store IS the application state. Every operation is an ontology operation. The Terraform lifecycle (plan/apply/monitor/drift/enforce) maps directly to the evaluation lifecycle.
