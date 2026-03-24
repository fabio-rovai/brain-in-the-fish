---
name: brain-in-the-fish
description: Universal document evaluation engine — evaluate any document against any criteria using cognitively-modelled AI agents with ontology-grounded scoring
version: 0.1.0
---

# Brain in the Fish — MCP Skill Guide

## What This Does

Brain in the Fish evaluates documents (essays, policies, contracts, clinical reports, surveys) against evaluation criteria using a panel of AI agents. Each agent's mental state exists as OWL ontology. Scoring is grounded in an Evidence Density Scorer (EDS) that makes hallucination mathematically detectable.

## MCP Tools Available

| Tool | Purpose | When to Call |
|------|---------|-------------|
| `eval_status` | Check server status and session state | First — verify server is running |
| `eval_ingest` | Ingest a document (PDF/text) | Step 1 |
| `eval_criteria` | Load evaluation framework | Step 2 |
| `eval_align` | Align document sections to criteria | Step 3 |
| `eval_spawn` | Generate evaluator agent panel | Step 4 |
| `eval_scoring_tasks` | Get all scoring prompts for subagents | Step 5 |
| `eval_score_prompt` | Get scoring prompt for one agent/criterion pair | Step 5 (per-task) |
| `eval_record_score` | Record a score from an agent | Step 6 |
| `eval_debate_status` | Check disagreements and convergence | Step 7 |
| `eval_challenge_prompt` | Get challenge prompt for debate | Step 7 (per-challenge) |
| `eval_report` | Generate final evaluation report | Step 8 |
| `eval_whatif` | "What if" re-scoring with modified text | Optional |

## Evaluation Workflow

### Quick Mode (deterministic, no subagents needed)

```
eval_ingest → eval_criteria → eval_align → eval_spawn → eval_report
```

The server runs evidence scoring internally. `eval_report` produces a complete evaluation with deterministic scores.

### Full Mode (with Claude subagent scoring)

```
1. eval_ingest(path, intent)
2. eval_criteria(framework_or_intent)
3. eval_align()
4. eval_spawn(intent)
5. eval_scoring_tasks() → get all tasks
6. For each task:
   - Read the scoring prompt
   - Evaluate the document content against the criterion as the agent persona
   - eval_record_score(agent_id, criterion_id, score, justification, evidence, gaps)
7. eval_debate_status() → check for disagreements
8. If disagreements:
   - eval_challenge_prompt(challenger, target, criterion)
   - Generate challenge argument
   - eval_record_score() with revised score
   - Repeat until converged
9. eval_report() → final report
```

### Subagent Dispatch Pattern

When orchestrating with multiple Claude subagents:

```
Orchestrator reads eval_scoring_tasks()
  → For each agent in the panel:
      Dispatch subagent with system prompt from eval_scoring_tasks
      Subagent receives: persona, criteria, document sections
      Subagent calls eval_record_score with their assessment
  → After all scores recorded:
      Check eval_debate_status
      If disagreements: dispatch challenge subagents
  → eval_report for final output
```

## Scoring Guidelines for Subagents

When scoring as an agent persona:

1. **Read the document content** provided in the scoring prompt carefully
2. **Reference the rubric levels** — state which level the document meets
3. **Cite specific evidence** from the document text (quote directly)
4. **Identify gaps** — what's missing that would improve the score
5. **Be the persona** — a Subject Expert scores differently from a Writing Specialist
6. **Do not hallucinate** — only reference evidence that appears in the provided text
7. **Use the full scale** — don't cluster all scores at 6-8. Use 1-10 range appropriately.

## Response Format for eval_record_score

```json
{
  "agent_id": "from the scoring task",
  "criterion_id": "from the scoring task",
  "score": 7.5,
  "max_score": 10.0,
  "round": 1,
  "justification": "Detailed justification referencing specific document content and rubric levels. This section meets Level 3 (score range 6-8) because it demonstrates [specific evidence]. To reach Level 4, the document would need [specific improvement].",
  "evidence_used": ["Direct quote from document", "Another quote"],
  "gaps_identified": ["Missing topic X", "No counter-argument for claim Y"]
}
```

## Supported Document Types

| Type | Intent Keywords | Framework Auto-Selected |
|------|----------------|----------------------|
| Academic essay | "essay", "mark", "grade", "coursework" | Academic Essay Marking |
| Policy document | "policy", "green book", "impact assessment" | HM Treasury Green Book |
| Survey/research | "survey", "methodology", "questionnaire" | Survey Methodology |
| Contract/legal | "contract", "legal", "compliance" | Contract Review |
| Clinical/NHS | "nhs", "clinical", "patient", "governance" | NHS Clinical Governance |
| GCSE English | "gcse", "english language" | GCSE English Language |
| Generic | anything else | Generic Quality |

## Architecture Notes

- **Three ontologies** coexist in one Oxigraph triple store: Document, Criteria, Agent
- **Evidence scorer** provides deterministic evidence-grounded scoring baseline
- **Validation signals** (citations, structure, reading level, fallacies, hedging) feed into the scorer as spikes
- **Epistemic state** tracks justified beliefs with empirical/normative/testimonial bases
- **Philosophical analysis** applies Kantian/utilitarian/virtue ethics lenses
- **Belief dynamics** — Maslow needs update based on findings, trust evolves during debate
- **Cross-evaluation memory** persists results for historical comparison
- **All triples are queryable** via SPARQL through the underlying onto_* tools
