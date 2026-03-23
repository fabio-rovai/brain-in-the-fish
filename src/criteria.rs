//! Evaluation criteria ontology loading and generation.

use crate::ingest::{iri_safe, turtle_escape};
use crate::types::{EvaluationCriterion, EvaluationFramework, RubricLevel};
use std::fmt::Write;

/// Convert an EvaluationFramework into Turtle RDF.
///
/// Uses prefixes:
///   @prefix crit: <http://brain-in-the-fish.dev/criteria/> .
///   @prefix eval: <http://brain-in-the-fish.dev/eval/> .
pub fn framework_to_turtle(framework: &EvaluationFramework) -> String {
    let mut out = String::new();

    // Prefixes
    let _ = writeln!(out, "@prefix crit: <http://brain-in-the-fish.dev/criteria/> .");
    let _ = writeln!(out, "@prefix eval: <http://brain-in-the-fish.dev/eval/> .");
    let _ = writeln!(out, "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .");
    let _ = writeln!(out);

    // Framework node
    let fw_id = iri_safe(&framework.id);
    let _ = writeln!(out, "crit:{fw_id} a eval:EvaluationFramework ;");
    let _ = writeln!(out, "    eval:name \"{}\" ;", turtle_escape(&framework.name));
    let _ = writeln!(
        out,
        "    eval:totalWeight \"{}\"^^xsd:decimal ;",
        framework.total_weight
    );
    let _ = writeln!(
        out,
        "    eval:passMark \"{}\"^^xsd:decimal .",
        framework.pass_mark.unwrap_or(0.0)
    );
    let _ = writeln!(out);

    // Criteria
    for criterion in &framework.criteria {
        write_criterion_turtle(&mut out, criterion, &fw_id);
    }

    out
}

/// Write Turtle triples for a single criterion and its rubric levels.
fn write_criterion_turtle(out: &mut String, criterion: &EvaluationCriterion, framework_id: &str) {
    let crit_id = iri_safe(&criterion.id);
    let _ = writeln!(out, "crit:{crit_id} a eval:EvaluationCriterion ;");
    let _ = writeln!(
        out,
        "    eval:title \"{}\" ;",
        turtle_escape(&criterion.title)
    );
    if let Some(ref desc) = criterion.description {
        let _ = writeln!(out, "    eval:description \"{}\" ;", turtle_escape(desc));
    }
    let _ = writeln!(
        out,
        "    eval:maxScore \"{}\"^^xsd:decimal ;",
        criterion.max_score
    );
    let _ = writeln!(
        out,
        "    eval:weight \"{}\"^^xsd:decimal ;",
        criterion.weight
    );
    let _ = writeln!(out, "    eval:partOf crit:{framework_id} .");
    let _ = writeln!(out);

    // Rubric levels
    for (i, level) in criterion.rubric_levels.iter().enumerate() {
        let level_id = format!("{}_L{}", crit_id, i);
        let _ = writeln!(out, "crit:{level_id} a eval:RubricLevel ;");
        let _ = writeln!(out, "    eval:criterion crit:{crit_id} ;");
        let _ = writeln!(out, "    eval:level \"{}\" ;", turtle_escape(&level.level));
        let _ = writeln!(
            out,
            "    eval:scoreRange \"{}\" ;",
            turtle_escape(&level.score_range)
        );
        let _ = writeln!(
            out,
            "    eval:descriptor \"{}\" .",
            turtle_escape(&level.descriptor)
        );
        let _ = writeln!(out);
    }

    // Sub-criteria (recursive)
    for sub in &criterion.sub_criteria {
        write_criterion_turtle(out, sub, framework_id);
    }
}

/// Load the criteria ontology into open-ontologies graph store.
pub fn load_criteria_ontology(
    graph: &open_ontologies::graph::GraphStore,
    framework: &EvaluationFramework,
) -> anyhow::Result<usize> {
    let turtle = framework_to_turtle(framework);
    let triples = graph.load_turtle(&turtle, None)?;
    Ok(triples)
}

/// Built-in generic quality framework for when no specific rubric is provided.
pub fn generic_quality_framework() -> EvaluationFramework {
    let rubric = |_: &str| -> Vec<RubricLevel> {
        vec![
            RubricLevel {
                level: "Excellent".to_string(),
                score_range: "9-10".to_string(),
                descriptor: "Outstanding quality demonstrating exceptional understanding and execution".to_string(),
            },
            RubricLevel {
                level: "Good".to_string(),
                score_range: "6-8".to_string(),
                descriptor: "Solid quality with clear competence and only minor gaps".to_string(),
            },
            RubricLevel {
                level: "Adequate".to_string(),
                score_range: "4-5".to_string(),
                descriptor: "Meets minimum requirements but lacks depth or detail".to_string(),
            },
            RubricLevel {
                level: "Poor".to_string(),
                score_range: "0-3".to_string(),
                descriptor: "Fails to meet requirements with significant gaps or errors".to_string(),
            },
        ]
    };

    let criteria = vec![
        EvaluationCriterion {
            id: "clarity_structure".to_string(),
            title: "Clarity & Structure".to_string(),
            description: Some("Logical organisation, clear writing, effective use of headings and formatting".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric("clarity"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "evidence_specificity".to_string(),
            title: "Evidence & Specificity".to_string(),
            description: Some("Use of concrete examples, case studies, data, and quantified outcomes".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric("evidence"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "technical_depth".to_string(),
            title: "Technical Depth".to_string(),
            description: Some("Demonstrates deep domain knowledge and sound methodology".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric("technical"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "feasibility_risk".to_string(),
            title: "Feasibility & Risk".to_string(),
            description: Some("Realistic planning, risk identification and mitigation strategies".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric("feasibility"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "value_for_money".to_string(),
            title: "Value for Money".to_string(),
            description: Some("Cost-effectiveness, efficient resource allocation, clear pricing rationale".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric("value"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "social_value".to_string(),
            title: "Social Value".to_string(),
            description: Some("Community benefit, sustainability, diversity and inclusion commitments".to_string()),
            max_score: 10.0,
            weight: 0.10,
            rubric_levels: rubric("social"),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "compliance".to_string(),
            title: "Compliance".to_string(),
            description: Some("Adherence to specification requirements, completeness of response".to_string()),
            max_score: 10.0,
            weight: 0.05,
            rubric_levels: rubric("compliance"),
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "generic_quality".to_string(),
        name: "Generic Quality Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(5.0),
        criteria,
    }
}

/// Built-in academic essay marking framework.
pub fn academic_essay_framework() -> EvaluationFramework {
    let criteria = vec![
        EvaluationCriterion {
            id: "knowledge_understanding".to_string(),
            title: "Knowledge & Understanding".to_string(),
            description: Some("Demonstrates comprehensive knowledge of the subject matter and key concepts".to_string()),
            max_score: 8.0,
            weight: 0.25,
            rubric_levels: vec![
                RubricLevel {
                    level: "Level 4".to_string(),
                    score_range: "7-8".to_string(),
                    descriptor: "Exceptional depth and breadth of knowledge with nuanced understanding".to_string(),
                },
                RubricLevel {
                    level: "Level 3".to_string(),
                    score_range: "5-6".to_string(),
                    descriptor: "Sound knowledge with clear understanding of key concepts".to_string(),
                },
                RubricLevel {
                    level: "Level 2".to_string(),
                    score_range: "3-4".to_string(),
                    descriptor: "Basic knowledge with some gaps in understanding".to_string(),
                },
                RubricLevel {
                    level: "Level 1".to_string(),
                    score_range: "0-2".to_string(),
                    descriptor: "Limited knowledge with significant misconceptions".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "analysis_evaluation".to_string(),
            title: "Analysis & Evaluation".to_string(),
            description: Some("Critical analysis, evaluation of arguments, and synthesis of ideas".to_string()),
            max_score: 12.0,
            weight: 0.30,
            rubric_levels: vec![
                RubricLevel {
                    level: "Level 4".to_string(),
                    score_range: "10-12".to_string(),
                    descriptor: "Sophisticated critical analysis with original insights and well-supported evaluation".to_string(),
                },
                RubricLevel {
                    level: "Level 3".to_string(),
                    score_range: "7-9".to_string(),
                    descriptor: "Competent analysis with clear evaluation of key arguments".to_string(),
                },
                RubricLevel {
                    level: "Level 2".to_string(),
                    score_range: "4-6".to_string(),
                    descriptor: "Some analysis attempted but largely descriptive".to_string(),
                },
                RubricLevel {
                    level: "Level 1".to_string(),
                    score_range: "0-3".to_string(),
                    descriptor: "Predominantly descriptive with minimal analysis".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "application".to_string(),
            title: "Application".to_string(),
            description: Some("Application of theory to practice, use of examples and real-world contexts".to_string()),
            max_score: 8.0,
            weight: 0.20,
            rubric_levels: vec![
                RubricLevel {
                    level: "Level 4".to_string(),
                    score_range: "7-8".to_string(),
                    descriptor: "Excellent application of theory with diverse and relevant examples".to_string(),
                },
                RubricLevel {
                    level: "Level 3".to_string(),
                    score_range: "5-6".to_string(),
                    descriptor: "Good application with appropriate examples".to_string(),
                },
                RubricLevel {
                    level: "Level 2".to_string(),
                    score_range: "3-4".to_string(),
                    descriptor: "Limited application with few or generic examples".to_string(),
                },
                RubricLevel {
                    level: "Level 1".to_string(),
                    score_range: "0-2".to_string(),
                    descriptor: "Fails to apply theory or provide meaningful examples".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "communication_structure".to_string(),
            title: "Communication & Structure".to_string(),
            description: Some("Clarity of expression, logical structure, academic writing conventions".to_string()),
            max_score: 6.0,
            weight: 0.15,
            rubric_levels: vec![
                RubricLevel {
                    level: "Level 4".to_string(),
                    score_range: "5-6".to_string(),
                    descriptor: "Articulate, well-structured with excellent academic style".to_string(),
                },
                RubricLevel {
                    level: "Level 3".to_string(),
                    score_range: "4".to_string(),
                    descriptor: "Clear writing with logical structure and appropriate style".to_string(),
                },
                RubricLevel {
                    level: "Level 2".to_string(),
                    score_range: "2-3".to_string(),
                    descriptor: "Understandable but with structural or stylistic weaknesses".to_string(),
                },
                RubricLevel {
                    level: "Level 1".to_string(),
                    score_range: "0-1".to_string(),
                    descriptor: "Poorly structured with unclear expression".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "use_of_sources".to_string(),
            title: "Use of Sources".to_string(),
            description: Some("Range and quality of sources, correct referencing and citation".to_string()),
            max_score: 6.0,
            weight: 0.10,
            rubric_levels: vec![
                RubricLevel {
                    level: "Level 4".to_string(),
                    score_range: "5-6".to_string(),
                    descriptor: "Wide range of authoritative sources with impeccable referencing".to_string(),
                },
                RubricLevel {
                    level: "Level 3".to_string(),
                    score_range: "4".to_string(),
                    descriptor: "Good range of sources with consistent referencing".to_string(),
                },
                RubricLevel {
                    level: "Level 2".to_string(),
                    score_range: "2-3".to_string(),
                    descriptor: "Limited sources with some referencing errors".to_string(),
                },
                RubricLevel {
                    level: "Level 1".to_string(),
                    score_range: "0-1".to_string(),
                    descriptor: "Few or no sources with poor or absent referencing".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "academic_essay".to_string(),
        name: "Academic Essay Marking Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(4.0),
        criteria,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framework_to_turtle() {
        let fw = generic_quality_framework();
        let turtle = framework_to_turtle(&fw);

        // Check prefixes
        assert!(turtle.contains("@prefix crit:"));
        assert!(turtle.contains("@prefix eval:"));
        assert!(turtle.contains("@prefix xsd:"));

        // Check framework node
        assert!(turtle.contains("eval:EvaluationFramework"));
        assert!(turtle.contains("eval:name \"Generic Quality Framework\""));
        assert!(turtle.contains("eval:totalWeight \"1\"^^xsd:decimal"));
        assert!(turtle.contains("eval:passMark \"5\"^^xsd:decimal"));

        // Check criteria
        assert!(turtle.contains("eval:EvaluationCriterion"));
        assert!(turtle.contains("eval:title \"Clarity & Structure\""));
        assert!(turtle.contains("eval:partOf crit:generic_quality"));

        // Check rubric levels
        assert!(turtle.contains("eval:RubricLevel"));
        assert!(turtle.contains("eval:level \"Excellent\""));
        assert!(turtle.contains("eval:scoreRange \"9-10\""));
        assert!(turtle.contains("eval:descriptor"));
    }

    #[test]
    fn test_load_criteria_ontology() {
        let graph = open_ontologies::graph::GraphStore::new();
        let fw = generic_quality_framework();
        let triples = load_criteria_ontology(&graph, &fw).expect("should load");

        // Framework: 4 triples (a, name, totalWeight, passMark)
        // Each criterion (7): 6 triples (a, title, description, maxScore, weight, partOf)
        // Each rubric level (7 * 4 = 28): 5 triples (a, criterion, level, scoreRange, descriptor)
        // Total: 4 + (7 * 6) + (28 * 5) = 4 + 42 + 140 = 186
        assert_eq!(triples, 186);
        assert_eq!(graph.triple_count(), 186);
    }

    #[test]
    fn test_generic_framework_weights_sum_to_1() {
        let fw = generic_quality_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "Weights should sum to 1.0, got {}",
            total
        );
    }

    #[test]
    fn test_academic_framework() {
        let fw = academic_essay_framework();
        assert_eq!(fw.criteria.len(), 5, "Should have 5 criteria");

        // All criteria should have rubric levels
        for criterion in &fw.criteria {
            assert_eq!(
                criterion.rubric_levels.len(),
                4,
                "Criterion '{}' should have 4 rubric levels",
                criterion.title
            );
        }

        // Weights should sum to 1.0
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "Academic framework weights should sum to 1.0, got {}",
            total
        );
    }
}
