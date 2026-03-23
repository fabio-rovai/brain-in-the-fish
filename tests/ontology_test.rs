//! Integration test: ontology generation and SPARQL querying.

use brain_in_the_fish::agent;
use brain_in_the_fish::criteria;
use brain_in_the_fish::ingest;
use brain_in_the_fish::types::*;
use open_ontologies::graph::GraphStore;

#[test]
fn test_document_ontology_sparql_queryable() {
    let graph = GraphStore::new();
    let doc = EvalDocument {
        id: "sparql-test".into(),
        title: "SPARQL Test Doc".into(),
        doc_type: "essay".into(),
        total_pages: None,
        total_word_count: Some(100),
        sections: vec![Section {
            id: "sec-1".into(),
            title: "Test Section".into(),
            text: "Some test content here".into(),
            word_count: 4,
            page_range: None,
            claims: vec![Claim {
                id: "c1".into(),
                text: "A test claim".into(),
                specificity: 0.5,
                verifiable: true,
            }],
            evidence: vec![],
            subsections: vec![],
        }],
    };

    ingest::load_document_ontology(&graph, &doc).unwrap();

    // Query for sections
    let result = graph
        .sparql_select(
            "PREFIX eval: <http://brain-in-the-fish.dev/eval/> SELECT ?s WHERE { ?s a eval:Section }",
        )
        .unwrap();
    assert!(
        result.contains("sec_1") || result.contains("sec-1"),
        "SPARQL should find the loaded section, got: {}",
        result
    );

    // Query for claims
    let result = graph
        .sparql_select(
            "PREFIX eval: <http://brain-in-the-fish.dev/eval/> SELECT ?c WHERE { ?c a eval:Claim }",
        )
        .unwrap();
    assert!(!result.is_empty(), "SPARQL should find claims");
}

#[test]
fn test_criteria_ontology_sparql_queryable() {
    let graph = GraphStore::new();
    let framework = criteria::generic_quality_framework();
    criteria::load_criteria_ontology(&graph, &framework).unwrap();

    let result = graph
        .sparql_select(
            "PREFIX eval: <http://brain-in-the-fish.dev/eval/> SELECT ?c WHERE { ?c a eval:EvaluationCriterion }",
        )
        .unwrap();
    // Should find 7 criteria — verify at least one is present
    assert!(
        result.contains("clarity") || result.contains("evidence"),
        "Should find a criterion in SPARQL results, got: {}",
        result
    );
}

#[test]
fn test_agent_ontology_sparql_queryable() {
    let graph = GraphStore::new();
    let framework = criteria::generic_quality_framework();
    let agents = agent::spawn_panel("evaluate this", &framework);

    for a in &agents {
        agent::load_agent_ontology(&graph, a).unwrap();
    }

    let result = graph
        .sparql_select(
            "PREFIX eval: <http://brain-in-the-fish.dev/eval/> SELECT ?a WHERE { ?a a eval:EvaluatorAgent }",
        )
        .unwrap();
    assert!(!result.is_empty(), "Should find loaded agents");
}

#[test]
fn test_all_three_ontologies_coexist() {
    // Load all three ontologies into the same graph and verify no conflicts
    let graph = GraphStore::new();

    let doc = EvalDocument {
        id: "coexist-doc".into(),
        title: "Coexistence Test".into(),
        doc_type: "essay".into(),
        total_pages: None,
        total_word_count: Some(50),
        sections: vec![Section {
            id: "s1".into(),
            title: "Section One".into(),
            text: "Content".into(),
            word_count: 1,
            page_range: None,
            claims: vec![],
            evidence: vec![],
            subsections: vec![],
        }],
    };

    let framework = criteria::academic_essay_framework();
    let agents = agent::spawn_panel("mark this essay", &framework);

    let doc_triples = ingest::load_document_ontology(&graph, &doc).unwrap();
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework).unwrap();
    let mut agent_triples = 0;
    for a in &agents {
        agent_triples += agent::load_agent_ontology(&graph, a).unwrap();
    }

    let total = doc_triples + crit_triples + agent_triples;

    // Verify total triple count is substantial
    assert!(
        total > 100,
        "Three ontologies should produce >100 triples, got {}",
        total
    );

    // Verify graph triple count matches what we loaded
    assert_eq!(
        graph.triple_count(),
        total,
        "Graph triple count should match loaded total"
    );
}
