//! Rule mining — derive facts from the argument graph via SPARQL.

use open_ontologies::graph::GraphStore;

use crate::types::{DerivedFacts, Document};

/// Mine derived facts from the argument graph loaded in the GraphStore.
///
/// Runs SPARQL INSERT rules then counts the resulting derived classes.
pub fn mine_facts(graph: &GraphStore, doc: &Document) -> DerivedFacts {
    // First, ensure the document is loaded (idempotent if already loaded)
    let _ = crate::ingest::load_document_ontology(graph, doc);

    // Run SPARQL INSERT rules
    let rules = default_rules();
    for rule in &rules {
        let _ = graph.sparql_update(rule);
    }

    // Count derived facts
    let count = |class: &str| -> usize {
        let query = format!(
            "PREFIX arg: <http://brain-in-the-fish.dev/arg/> \
             SELECT (COUNT(?x) AS ?c) WHERE {{ ?x a arg:{class} }}"
        );
        match graph.sparql_select(&query) {
            Ok(result) => {
                let v: serde_json::Value = serde_json::from_str(&result).unwrap_or_default();
                v["results"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|row| row["c"].as_str())
                    .and_then(parse_xsd_integer)
                    .unwrap_or(0)
            }
            Err(_) => 0,
        }
    };

    let strong = count("StrongClaim");
    let weak = count("WeakClaim");
    let unsupported = count("UnsupportedClaim");
    let quantified = count("QuantifiedSupport");
    let citation = count("CitationSupport");
    let deep = count("DeepChain");
    let sophisticated = count("SophisticatedArgument");
    let evidenced = count("EvidencedThesis") > 0;

    DerivedFacts {
        strong_claims: strong,
        weak_claims: weak,
        unsupported_claims: unsupported,
        supported_claims: strong + weak,
        sophisticated_arguments: sophisticated,
        circular_arguments: 0,
        evidenced_thesis: evidenced,
        unevidenced_thesis: !evidenced,
        quantified_support: quantified,
        citation_support: citation,
        deep_chains: deep,
    }
}

/// Convert derived facts into a feature vector for scoring.
pub fn facts_to_features(facts: &DerivedFacts, total_claims: usize) -> Vec<f64> {
    let tc = total_claims.max(1) as f64;
    vec![
        facts.strong_claims as f64 / tc,
        facts.weak_claims as f64 / tc,
        facts.unsupported_claims as f64 / tc,
        if facts.evidenced_thesis { 1.0 } else { 0.0 },
        if facts.sophisticated_arguments > 0 {
            1.0
        } else {
            0.0
        },
        facts.quantified_support as f64,
        facts.citation_support as f64,
        facts.deep_chains as f64,
    ]
}

// ---- internals ----

fn default_rules() -> Vec<String> {
    vec![
        // StrongClaim: claim with 2+ evidence
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?claim a arg:StrongClaim }
           WHERE {
               { ?claim a arg:SubClaim } UNION { ?claim a arg:Thesis }
               ?ev1 arg:supports ?claim .
               ?ev2 arg:supports ?claim .
               FILTER(?ev1 != ?ev2)
               { ?ev1 a arg:Evidence } UNION { ?ev1 a arg:QuantifiedEvidence } UNION { ?ev1 a arg:Citation }
               { ?ev2 a arg:Evidence } UNION { ?ev2 a arg:QuantifiedEvidence } UNION { ?ev2 a arg:Citation }
           }"#.into(),
        // WeakClaim: claim with exactly 1 evidence
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?claim a arg:WeakClaim }
           WHERE {
               { ?claim a arg:SubClaim } UNION { ?claim a arg:Thesis }
               ?ev arg:supports ?claim .
               { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
               FILTER NOT EXISTS {
                   ?ev2 arg:supports ?claim .
                   FILTER(?ev2 != ?ev)
                   { ?ev2 a arg:Evidence } UNION { ?ev2 a arg:QuantifiedEvidence } UNION { ?ev2 a arg:Citation }
               }
           }"#.into(),
        // UnsupportedClaim
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?claim a arg:UnsupportedClaim }
           WHERE {
               { ?claim a arg:SubClaim } UNION { ?claim a arg:Thesis }
               FILTER NOT EXISTS {
                   ?ev arg:supports ?claim .
                   { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
               }
           }"#.into(),
        // SophisticatedArgument
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?thesis a arg:SophisticatedArgument }
           WHERE {
               ?thesis a arg:Thesis .
               ?counter arg:counters ?thesis .
               ?rebuttal arg:rebuts ?counter .
           }"#.into(),
        // EvidencedThesis
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?thesis a arg:EvidencedThesis }
           WHERE {
               ?thesis a arg:Thesis .
               ?x arg:supports ?thesis .
               { ?x a arg:Evidence } UNION { ?x a arg:QuantifiedEvidence } UNION { ?x a arg:Citation }
               UNION {
                   ?x arg:supports ?thesis .
                   ?ev arg:supports ?x .
                   { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
               }
           }"#.into(),
        // QuantifiedSupport
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?ev a arg:QuantifiedSupport }
           WHERE {
               ?ev a arg:QuantifiedEvidence .
               ?ev arg:supports ?claim .
           }"#.into(),
        // CitationSupport
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?ev a arg:CitationSupport }
           WHERE {
               ?ev a arg:Citation .
               ?ev arg:supports ?claim .
           }"#.into(),
        // DeepChain
        r#"PREFIX arg: <http://brain-in-the-fish.dev/arg/>
           INSERT { ?ev a arg:DeepChain }
           WHERE {
               ?ev arg:supports ?mid .
               ?mid arg:supports ?thesis .
               ?thesis a arg:Thesis .
               { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
           }"#.into(),
    ]
}

fn parse_xsd_integer(s: &str) -> Option<usize> {
    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }
    if let Some(start) = s.find('"') {
        let rest = &s[start + 1..];
        if let Some(end) = rest.find('"') {
            return rest[..end].parse::<usize>().ok();
        }
    }
    None
}
