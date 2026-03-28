//! Rule mining over argument graphs using SPARQL CONSTRUCT/INSERT rules.
//!
//! Rules derive new facts from the OWL knowledge graph — e.g., a claim with 2+
//! supporting evidence nodes becomes a `StrongClaim`. These derived facts become
//! features for scoring: deterministic, auditable, and grounded in graph topology.

use open_ontologies::graph::GraphStore;
use serde::{Deserialize, Serialize};

/// A SPARQL INSERT rule that derives new facts from the argument graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Human-readable name for the rule.
    pub name: String,
    /// What this rule detects.
    pub description: String,
    /// SPARQL UPDATE (INSERT WHERE) query.
    pub sparql: String,
}

/// Facts derived by running rules over the graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DerivedFacts {
    pub strong_claims: usize,
    pub weak_claims: usize,
    pub unsupported_claims: usize,
    pub supported_claims: usize,
    pub sophisticated_arguments: usize,
    pub circular_arguments: usize,
    pub evidenced_thesis: bool,
    pub unevidenced_thesis: bool,
    pub quantified_support: usize,
    pub citation_support: usize,
    pub deep_chains: usize,
}

/// The default rule set for argument evaluation.
pub fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "StrongClaim".into(),
            description: "A claim with 2+ supporting evidence nodes".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?claim a arg:StrongClaim }
                WHERE {
                    { ?claim a arg:SubClaim } UNION { ?claim a arg:Thesis }
                    ?ev1 arg:supports ?claim .
                    ?ev2 arg:supports ?claim .
                    FILTER(?ev1 != ?ev2)
                    { ?ev1 a arg:Evidence } UNION { ?ev1 a arg:QuantifiedEvidence } UNION { ?ev1 a arg:Citation }
                    { ?ev2 a arg:Evidence } UNION { ?ev2 a arg:QuantifiedEvidence } UNION { ?ev2 a arg:Citation }
                }
            "#.into(),
        },
        Rule {
            name: "WeakClaim".into(),
            description: "A claim with exactly 1 supporting evidence node".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
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
                }
            "#.into(),
        },
        Rule {
            name: "UnsupportedClaim".into(),
            description: "A claim with zero supporting evidence".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?claim a arg:UnsupportedClaim }
                WHERE {
                    { ?claim a arg:SubClaim } UNION { ?claim a arg:Thesis }
                    FILTER NOT EXISTS {
                        ?ev arg:supports ?claim .
                        { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
                    }
                }
            "#.into(),
        },
        Rule {
            name: "SophisticatedArgument".into(),
            description: "Thesis with counter-argument that has a rebuttal".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?thesis a arg:SophisticatedArgument }
                WHERE {
                    ?thesis a arg:Thesis .
                    ?counter arg:counters ?thesis .
                    ?rebuttal arg:rebuts ?counter .
                }
            "#.into(),
        },
        Rule {
            name: "EvidencedThesis".into(),
            description: "Thesis directly or transitively supported by evidence".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
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
                }
            "#.into(),
        },
        Rule {
            name: "QuantifiedSupport".into(),
            description: "Evidence node with quantified data supporting a claim".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?ev a arg:QuantifiedSupport }
                WHERE {
                    ?ev a arg:QuantifiedEvidence .
                    ?ev arg:supports ?claim .
                }
            "#.into(),
        },
        Rule {
            name: "CitationSupport".into(),
            description: "Citation node supporting a claim".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?ev a arg:CitationSupport }
                WHERE {
                    ?ev a arg:Citation .
                    ?ev arg:supports ?claim .
                }
            "#.into(),
        },
        Rule {
            name: "DeepChain".into(),
            description: "Evidence supporting a claim that supports the thesis (depth >= 2)".into(),
            sparql: r#"
                PREFIX arg: <http://brain-in-the-fish.dev/arg/>
                INSERT { ?ev a arg:DeepChain }
                WHERE {
                    ?ev arg:supports ?mid .
                    ?mid arg:supports ?thesis .
                    ?thesis a arg:Thesis .
                    { ?ev a arg:Evidence } UNION { ?ev a arg:QuantifiedEvidence } UNION { ?ev a arg:Citation }
                }
            "#.into(),
        },
    ]
}

/// Run all rules against a graph and count derived facts.
pub fn mine_facts(store: &GraphStore, rules: &[Rule]) -> anyhow::Result<DerivedFacts> {
    // Execute each rule
    for rule in rules {
        if let Err(e) = store.sparql_update(&rule.sparql) {
            tracing::warn!("Rule '{}' failed: {}", rule.name, e);
        }
    }

    // Count derived facts via SPARQL
    let mut facts = DerivedFacts::default();

    let count = |class: &str| -> usize {
        let query = format!(
            "PREFIX arg: <http://brain-in-the-fish.dev/arg/> SELECT (COUNT(?x) AS ?c) WHERE {{ ?x a arg:{} }}",
            class
        );
        match store.sparql_select(&query) {
            Ok(result) => {
                // Parse count from SPARQL result
                let v: serde_json::Value = serde_json::from_str(&result).unwrap_or_default();
                v["results"].as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|row| row["c"].as_str())
                    .and_then(|s| s.trim_matches('"').parse::<usize>().ok())
                    .or_else(|| {
                        // Try bindings format
                        v["results"]["bindings"].as_array()
                            .and_then(|arr| arr.first())
                            .and_then(|row| row["c"]["value"].as_str())
                            .and_then(|s| s.parse::<usize>().ok())
                    })
                    .unwrap_or(0)
            }
            Err(_) => 0,
        }
    };

    let ask = |class: &str| -> bool {
        count(class) > 0
    };

    facts.strong_claims = count("StrongClaim");
    facts.weak_claims = count("WeakClaim");
    facts.unsupported_claims = count("UnsupportedClaim");
    facts.supported_claims = facts.strong_claims + facts.weak_claims;
    facts.sophisticated_arguments = count("SophisticatedArgument");
    facts.evidenced_thesis = ask("EvidencedThesis");
    facts.unevidenced_thesis = !facts.evidenced_thesis;
    facts.quantified_support = count("QuantifiedSupport");
    facts.citation_support = count("CitationSupport");
    facts.deep_chains = count("DeepChain");

    Ok(facts)
}

/// Convert derived facts into a feature vector for scoring.
pub fn facts_to_features(facts: &DerivedFacts, total_claims: usize) -> Vec<f64> {
    let tc = total_claims.max(1) as f64;
    vec![
        facts.strong_claims as f64 / tc,           // fraction of strong claims
        facts.weak_claims as f64 / tc,              // fraction of weak claims
        facts.unsupported_claims as f64 / tc,       // fraction of unsupported claims
        if facts.evidenced_thesis { 1.0 } else { 0.0 },
        if facts.sophisticated_arguments > 0 { 1.0 } else { 0.0 },
        facts.quantified_support as f64,            // count of quantified evidence
        facts.citation_support as f64,              // count of citations
        facts.deep_chains as f64,                   // count of depth-2+ chains
    ]
}
