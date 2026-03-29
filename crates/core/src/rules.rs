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

/// Parse an XSD integer literal from Oxigraph's term string representation.
///
/// Oxigraph renders COUNT results as `"N"^^<http://www.w3.org/2001/XMLSchema#integer>`
/// (or #decimal, #long, etc.). This extracts the numeric portion.
fn parse_xsd_integer(s: &str) -> Option<usize> {
    // Try direct parse first (plain number)
    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }
    // Handle XSD literal format: "N"^^<datatype>
    if let Some(start) = s.find('"') {
        let rest = &s[start + 1..];
        if let Some(end) = rest.find('"') {
            return rest[..end].parse::<usize>().ok();
        }
    }
    None
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
                // Oxigraph sparql_select returns:
                //   {"variables":["c"],"results":[{"c":"\"0\"^^<http://www.w3.org/2001/XMLSchema#integer>"}]}
                // We need to extract the numeric value from the XSD literal string.
                v["results"].as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|row| row["c"].as_str())
                    .and_then(|s| parse_xsd_integer(s))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal argument graph with a thesis, two sub-claims, evidence, a counter, and rebuttal.
    const TEST_GRAPH: &str = r#"
        @prefix arg: <http://brain-in-the-fish.dev/arg/> .

        # Thesis
        arg:thesis1 a arg:Thesis ;
            arg:hasText "AI will transform education" .

        # Sub-claims
        arg:claim1 a arg:SubClaim ;
            arg:hasText "AI tutors improve test scores" ;
            arg:supports arg:thesis1 .

        arg:claim2 a arg:SubClaim ;
            arg:hasText "AI reduces teacher workload" ;
            arg:supports arg:thesis1 .

        arg:claim3 a arg:SubClaim ;
            arg:hasText "AI promotes creativity" .

        # Evidence (claim1 has 2 evidence nodes -> StrongClaim)
        arg:ev1 a arg:Evidence ;
            arg:hasText "Study showed 15% improvement" ;
            arg:supports arg:claim1 .

        arg:ev2 a arg:QuantifiedEvidence ;
            arg:hasText "N=500, p<0.01, effect size 0.8" ;
            arg:supports arg:claim1 .

        # Evidence (claim2 has 1 evidence node -> WeakClaim)
        arg:ev3 a arg:Citation ;
            arg:hasText "Smith et al. 2024" ;
            arg:supports arg:claim2 .

        # claim3 has no evidence -> UnsupportedClaim

        # Counter-argument with rebuttal -> SophisticatedArgument
        arg:counter1 a arg:Counter ;
            arg:hasText "AI may increase inequality" ;
            arg:counters arg:thesis1 .

        arg:rebuttal1 a arg:Rebuttal ;
            arg:hasText "Open-source AI levels the field" ;
            arg:rebuts arg:counter1 .

        # Deep chain: ev1 -> claim1 -> thesis1 (depth 2)
        # (already present via arg:supports links above)
    "#;

    #[test]
    fn test_parse_xsd_integer() {
        // Plain number
        assert_eq!(parse_xsd_integer("3"), Some(3));
        // XSD integer literal as Oxigraph renders it
        assert_eq!(
            parse_xsd_integer("\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            Some(3)
        );
        assert_eq!(
            parse_xsd_integer("\"0\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            Some(0)
        );
        assert_eq!(
            parse_xsd_integer("\"12\"^^<http://www.w3.org/2001/XMLSchema#decimal>"),
            Some(12)
        );
        // Garbage
        assert_eq!(parse_xsd_integer("not_a_number"), None);
    }

    #[test]
    fn test_default_rules_count() {
        let rules = default_rules();
        assert_eq!(rules.len(), 8, "Expected 8 default rules");
    }

    #[test]
    fn test_sparql_insert_rules_against_real_graphstore() {
        let store = GraphStore::new();
        let loaded = store.load_turtle(TEST_GRAPH, None).expect("Failed to load test graph");
        assert!(loaded > 0, "Should load triples from test graph");

        let rules = default_rules();
        let facts = mine_facts(&store, &rules).expect("mine_facts should succeed");

        // claim1 has 2 evidence nodes (ev1, ev2) -> StrongClaim
        assert_eq!(facts.strong_claims, 1, "claim1 should be the only StrongClaim");

        // claim2 has exactly 1 evidence node (ev3) -> WeakClaim
        assert_eq!(facts.weak_claims, 1, "claim2 should be the only WeakClaim");

        // claim3 has no evidence -> UnsupportedClaim
        // thesis1 also has no *direct* evidence nodes (claims support it, not Evidence)
        // So thesis1 should also be UnsupportedClaim by the rule definition.
        assert!(
            facts.unsupported_claims >= 1,
            "claim3 (and possibly thesis1) should be UnsupportedClaim, got {}",
            facts.unsupported_claims
        );

        // supported_claims = strong + weak
        assert_eq!(facts.supported_claims, facts.strong_claims + facts.weak_claims);

        // counter1 counters thesis1 and rebuttal1 rebuts counter1 -> SophisticatedArgument
        assert_eq!(facts.sophisticated_arguments, 1, "thesis1 should be SophisticatedArgument");

        // EvidencedThesis: thesis1 has subclaims supporting it, but the rule checks for
        // Evidence/QuantifiedEvidence/Citation directly or transitively supporting the thesis.
        // claim1 supports thesis1 (but claim1 is a SubClaim, not Evidence).
        // The transitive branch: ev arg:supports x, x arg:supports thesis -> but ev1 supports claim1,
        // and claim1 supports thesis1. ev1 is Evidence. So this should match.
        assert!(facts.evidenced_thesis, "thesis1 should be EvidencedThesis via transitive support");
        assert!(!facts.unevidenced_thesis);

        // QuantifiedSupport: ev2 is QuantifiedEvidence and supports claim1
        assert_eq!(facts.quantified_support, 1, "ev2 should be QuantifiedSupport");

        // CitationSupport: ev3 is Citation and supports claim2
        assert_eq!(facts.citation_support, 1, "ev3 should be CitationSupport");

        // DeepChain: ev1 -> claim1 -> thesis1 and ev2 -> claim1 -> thesis1
        // Also ev3 -> claim2 -> thesis1
        assert_eq!(facts.deep_chains, 3, "ev1, ev2, ev3 all form deep chains through claims to thesis");
    }

    #[test]
    fn test_facts_to_features_normalisation() {
        let facts = DerivedFacts {
            strong_claims: 2,
            weak_claims: 1,
            unsupported_claims: 1,
            supported_claims: 3,
            sophisticated_arguments: 1,
            circular_arguments: 0,
            evidenced_thesis: true,
            unevidenced_thesis: false,
            quantified_support: 2,
            citation_support: 1,
            deep_chains: 3,
        };
        let features = facts_to_features(&facts, 4);
        assert_eq!(features.len(), 8);
        assert!((features[0] - 0.5).abs() < 1e-9, "strong/total = 2/4 = 0.5");
        assert!((features[1] - 0.25).abs() < 1e-9, "weak/total = 1/4 = 0.25");
        assert!((features[2] - 0.25).abs() < 1e-9, "unsupported/total = 1/4 = 0.25");
        assert!((features[3] - 1.0).abs() < 1e-9, "evidenced thesis = 1.0");
        assert!((features[4] - 1.0).abs() < 1e-9, "sophisticated > 0 = 1.0");
    }

    #[test]
    fn test_empty_graph_returns_defaults() {
        let store = GraphStore::new();
        let rules = default_rules();
        let facts = mine_facts(&store, &rules).expect("mine_facts on empty graph should succeed");

        assert_eq!(facts.strong_claims, 0);
        assert_eq!(facts.weak_claims, 0);
        assert_eq!(facts.unsupported_claims, 0);
        assert!(!facts.evidenced_thesis);
        assert!(facts.unevidenced_thesis);
    }
}
