# Tardygrada <-> Open-Ontologies Wire Protocol

## Transport
Unix domain socket, SOCK_STREAM.
Messages are newline-delimited JSON (one JSON object per line).

Default socket paths:
- Sketch: `/tmp/tardygrada-ontology-sketch.sock`
- Complete: `/tmp/tardygrada-ontology-complete.sock`

## Requests

### Ground Triples

Check if triples are supported by the knowledge graph.

Request:
```json
{"action": "ground", "triples": [
  {"s": "DrWho", "p": "created_at", "o": "BBCTelevisionCentre"},
  {"s": "DrWho", "p": "created_year", "o": "1963"}
]}
```

Response:
```json
{"results": [
  {"status": "grounded", "confidence": 95, "evidence_count": 3},
  {"status": "grounded", "confidence": 88, "evidence_count": 1}
]}
```

Status values:
- `"grounded"` — ontology contains supporting triples
- `"unknown"` — ontology has no opinion (not necessarily wrong)
- `"contradicted"` — ontology contains evidence against this claim

Confidence: integer 0-100.

### Check Consistency

Check if a set of triples is internally consistent with the ontology.

Request:
```json
{"action": "check_consistency", "triples": [
  {"s": "DrWho", "p": "created_at", "o": "BBCTelevisionCentre"},
  {"s": "DrWho", "p": "created_at", "o": "Tokyo"}
]}
```

Response:
```json
{"consistent": false, "contradiction_count": 1, "explanation": "DrWho.created_at has conflicting values"}
```

## Fallback Behavior

When the ontology engine is unavailable:
- All triples are marked `"unknown"` (honest uncertainty)
- Consistency defaults to `true` (can't prove inconsistency without data)
- The verification pipeline still runs but with lower confidence

## Dual-Mode Operation

Tardygrada connects to two ontology instances:
1. **Sketch** (fast, permissive) — fewer triples, faster reasoning
2. **Complete** (slow, strict) — full knowledge graph

If sketch detects contradictions, fail fast without querying complete.
If both pass, high confidence. If only one is available, use it alone.

## Open-Ontologies Adapter

The open-ontologies Rust project needs a thin adapter:
1. Listen on unix socket
2. Parse incoming JSON
3. For "ground": run SPARQL-like queries against the triple store
4. For "check_consistency": run OWL consistency reasoning
5. Return JSON response

The reasoner in `open-ontologies/src/reason.rs` already supports:
- RDFS subclass/subproperty reasoning
- OWL transitive, symmetric, inverse properties
- OWL sameAs, equivalentClass
- OWL someValuesFrom, allValuesFrom restrictions
- OWL intersection/union
