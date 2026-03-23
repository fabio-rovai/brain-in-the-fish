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

/// Built-in HM Treasury Green Book impact assessment framework.
pub fn policy_greenbook_framework() -> EvaluationFramework {
    let rubric = |desc_outstanding: &str, desc_good: &str, desc_adequate: &str, desc_inadequate: &str| -> Vec<RubricLevel> {
        vec![
            RubricLevel {
                level: "Outstanding".to_string(),
                score_range: "9-10".to_string(),
                descriptor: desc_outstanding.to_string(),
            },
            RubricLevel {
                level: "Good".to_string(),
                score_range: "6-8".to_string(),
                descriptor: desc_good.to_string(),
            },
            RubricLevel {
                level: "Adequate".to_string(),
                score_range: "4-5".to_string(),
                descriptor: desc_adequate.to_string(),
            },
            RubricLevel {
                level: "Inadequate".to_string(),
                score_range: "0-3".to_string(),
                descriptor: desc_inadequate.to_string(),
            },
        ]
    };

    let criteria = vec![
        EvaluationCriterion {
            id: "rationale_objectives".to_string(),
            title: "Rationale & Objectives".to_string(),
            description: Some("Is the rationale clear? Are SMART objectives defined?".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Compelling rationale with fully SMART objectives clearly linked to policy need",
                "Clear rationale with well-defined objectives that are mostly SMART",
                "Rationale present but objectives lack specificity or measurability",
                "Weak or missing rationale with vague or absent objectives",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "evidence_base".to_string(),
            title: "Evidence Base".to_string(),
            description: Some("Quality and recency of evidence cited".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Comprehensive, recent, and authoritative evidence base with systematic review",
                "Good range of current evidence from credible sources",
                "Some evidence cited but gaps in coverage or currency",
                "Little or no evidence, or reliance on outdated or unreliable sources",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "options_appraisal".to_string(),
            title: "Options Appraisal".to_string(),
            description: Some("Are alternatives properly considered?".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Thorough appraisal of all viable options including do-nothing with clear rationale for preferred option",
                "Good range of options considered with reasonable comparative analysis",
                "Limited options considered or superficial comparison",
                "No meaningful options appraisal or only one option presented",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "stakeholder_engagement".to_string(),
            title: "Stakeholder Engagement".to_string(),
            description: Some("Are affected communities represented?".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Extensive, inclusive engagement with clear evidence of how input shaped proposals",
                "Good engagement with key stakeholder groups and evidence of influence on design",
                "Some engagement undertaken but limited scope or unclear how it informed proposals",
                "No meaningful stakeholder engagement or consultation",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "value_for_money_greenbook".to_string(),
            title: "Value for Money".to_string(),
            description: Some("Is the cost-benefit analysis sound?".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Rigorous CBA with sensitivity analysis, monetised and non-monetised benefits clearly presented",
                "Sound CBA with appropriate methodology and reasonable assumptions",
                "Basic cost-benefit consideration but lacking rigour or sensitivity testing",
                "No meaningful CBA or fundamentally flawed economic analysis",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "delivery_implementation".to_string(),
            title: "Delivery & Implementation".to_string(),
            description: Some("Is the plan feasible and realistic?".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Detailed, realistic delivery plan with clear milestones, risks, and contingencies",
                "Good delivery plan with reasonable timelines and identified risks",
                "Basic plan present but lacking detail on risks or contingencies",
                "No credible delivery plan or fundamentally unrealistic proposals",
            ),
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "policy_greenbook".to_string(),
        name: "HM Treasury Green Book Impact Assessment".to_string(),
        total_weight: 1.0,
        pass_mark: Some(60.0),
        criteria,
    }
}

/// Built-in research survey quality framework.
pub fn survey_methodology_framework() -> EvaluationFramework {
    let rubric = |desc_outstanding: &str, desc_good: &str, desc_adequate: &str, desc_inadequate: &str| -> Vec<RubricLevel> {
        vec![
            RubricLevel {
                level: "Outstanding".to_string(),
                score_range: "9-10".to_string(),
                descriptor: desc_outstanding.to_string(),
            },
            RubricLevel {
                level: "Good".to_string(),
                score_range: "6-8".to_string(),
                descriptor: desc_good.to_string(),
            },
            RubricLevel {
                level: "Adequate".to_string(),
                score_range: "4-5".to_string(),
                descriptor: desc_adequate.to_string(),
            },
            RubricLevel {
                level: "Inadequate".to_string(),
                score_range: "0-3".to_string(),
                descriptor: desc_inadequate.to_string(),
            },
        ]
    };

    let criteria = vec![
        EvaluationCriterion {
            id: "sampling_design".to_string(),
            title: "Sampling Design".to_string(),
            description: Some("Representative, adequate size, appropriate method".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Rigorous probability sampling with justified sample size and power analysis",
                "Appropriate sampling method with adequate sample size for the research questions",
                "Basic sampling approach but with gaps in justification or representativeness",
                "No clear sampling strategy or fundamentally flawed sample design",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "question_design".to_string(),
            title: "Question Design".to_string(),
            description: Some("Clear, unbiased, valid, reliable".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Well-crafted questions with validated scales, no bias, and excellent face validity",
                "Clear questions with appropriate response options and minimal bias",
                "Functional questions but some ambiguity, leading language, or validity concerns",
                "Poorly constructed questions with significant bias or validity issues",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "data_collection".to_string(),
            title: "Data Collection".to_string(),
            description: Some("Consistent procedures, response rates".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Standardised procedures with high response rates and robust non-response analysis",
                "Consistent collection methods with acceptable response rates",
                "Basic procedures in place but inconsistencies or low response rates",
                "No standardised procedures or critically low response rates",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "analysis_methods".to_string(),
            title: "Analysis Methods".to_string(),
            description: Some("Appropriate statistics, significance testing".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Sophisticated and appropriate statistical methods with effect sizes and confidence intervals",
                "Sound analytical approach with appropriate significance testing",
                "Basic analysis present but methods may not fully suit the data or research questions",
                "Inappropriate or absent statistical analysis",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "ethics_consent".to_string(),
            title: "Ethics & Consent".to_string(),
            description: Some("Informed consent, data protection, anonymisation".to_string()),
            max_score: 10.0,
            weight: 0.10,
            rubric_levels: rubric(
                "Exemplary ethical framework with clear consent, full GDPR compliance, and robust anonymisation",
                "Good ethical procedures with appropriate consent and data protection measures",
                "Basic ethical considerations addressed but gaps in consent or data protection",
                "Significant ethical concerns or absent consent and data protection procedures",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "reporting".to_string(),
            title: "Reporting".to_string(),
            description: Some("Limitations acknowledged, findings clearly presented".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Transparent reporting with full limitations, clear visualisations, and actionable findings",
                "Clear presentation of findings with limitations acknowledged",
                "Findings presented but limitations understated or presentation unclear",
                "Poor reporting with no limitations discussed or findings unclear",
            ),
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "survey_methodology".to_string(),
        name: "Research Survey Quality Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(50.0),
        criteria,
    }
}

/// Built-in legal contract assessment framework.
pub fn contract_review_framework() -> EvaluationFramework {
    let rubric = |desc_outstanding: &str, desc_good: &str, desc_adequate: &str, desc_inadequate: &str| -> Vec<RubricLevel> {
        vec![
            RubricLevel {
                level: "Outstanding".to_string(),
                score_range: "9-10".to_string(),
                descriptor: desc_outstanding.to_string(),
            },
            RubricLevel {
                level: "Good".to_string(),
                score_range: "6-8".to_string(),
                descriptor: desc_good.to_string(),
            },
            RubricLevel {
                level: "Adequate".to_string(),
                score_range: "4-5".to_string(),
                descriptor: desc_adequate.to_string(),
            },
            RubricLevel {
                level: "Inadequate".to_string(),
                score_range: "0-3".to_string(),
                descriptor: desc_inadequate.to_string(),
            },
        ]
    };

    let criteria = vec![
        EvaluationCriterion {
            id: "clarity_of_terms".to_string(),
            title: "Clarity of Terms".to_string(),
            description: Some("Definitions, unambiguous language".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Comprehensive definitions section with precise, unambiguous language throughout",
                "Good definitions with generally clear language and few ambiguities",
                "Some definitions present but language creates potential for misinterpretation",
                "Missing definitions or pervasively ambiguous language",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "obligations_deliverables".to_string(),
            title: "Obligations & Deliverables".to_string(),
            description: Some("Clear scope, milestones, acceptance criteria".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Precisely defined obligations with measurable deliverables, milestones, and acceptance criteria",
                "Clear obligations and deliverables with reasonable acceptance criteria",
                "Obligations stated but deliverables lack specificity or acceptance criteria",
                "Vague or missing obligations with no clear deliverables or acceptance criteria",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "risk_allocation".to_string(),
            title: "Risk Allocation".to_string(),
            description: Some("Liability, indemnities, force majeure".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Balanced risk allocation with appropriate liability caps, mutual indemnities, and comprehensive force majeure",
                "Reasonable risk allocation with adequate liability provisions and force majeure clause",
                "Risk allocation present but imbalanced or with gaps in coverage",
                "One-sided risk allocation, missing liability caps, or absent force majeure provisions",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "commercial_terms".to_string(),
            title: "Commercial Terms".to_string(),
            description: Some("Pricing, payment, penalties".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Transparent pricing with fair payment terms, proportionate penalties, and clear variation mechanisms",
                "Clear pricing and payment terms with reasonable penalty provisions",
                "Basic commercial terms present but lacking clarity on variations or penalties",
                "Unclear pricing, unreasonable payment terms, or disproportionate penalties",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "regulatory_compliance".to_string(),
            title: "Regulatory Compliance".to_string(),
            description: Some("GDPR, sector-specific regulations".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Full regulatory compliance with GDPR data processing agreement, sector-specific provisions, and audit rights",
                "Good regulatory awareness with appropriate GDPR and sector-specific clauses",
                "Basic compliance clauses present but gaps in GDPR or sector-specific requirements",
                "Missing or inadequate regulatory compliance provisions",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "termination_dispute".to_string(),
            title: "Termination & Dispute".to_string(),
            description: Some("Exit clauses, dispute resolution".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Clear termination rights for both parties with graduated dispute resolution including mediation and arbitration",
                "Reasonable termination provisions with a defined dispute resolution process",
                "Basic exit clauses present but dispute resolution mechanism is unclear",
                "Missing termination rights or no dispute resolution mechanism",
            ),
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "contract_review".to_string(),
        name: "Legal Contract Assessment Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(60.0),
        criteria,
    }
}

/// Built-in GCSE English Language marking framework.
pub fn gcse_english_framework() -> EvaluationFramework {
    let criteria = vec![
        EvaluationCriterion {
            id: "content_ideas".to_string(),
            title: "Content & Ideas".to_string(),
            description: Some("Engaging content, original ideas, deliberate choices".to_string()),
            max_score: 24.0,
            weight: 0.25,
            rubric_levels: vec![
                RubricLevel {
                    level: "Band 5".to_string(),
                    score_range: "21-24".to_string(),
                    descriptor: "Compelling, convincing content with extensive and ambitious original ideas and deliberate, controlled choices".to_string(),
                },
                RubricLevel {
                    level: "Band 4".to_string(),
                    score_range: "16-20".to_string(),
                    descriptor: "Engaging content with well-developed ideas and conscious crafting choices".to_string(),
                },
                RubricLevel {
                    level: "Band 3".to_string(),
                    score_range: "11-15".to_string(),
                    descriptor: "Clear content with some developed ideas and evidence of deliberate choices".to_string(),
                },
                RubricLevel {
                    level: "Band 2".to_string(),
                    score_range: "6-10".to_string(),
                    descriptor: "Some relevant content with simple ideas and occasional deliberate choices".to_string(),
                },
                RubricLevel {
                    level: "Band 1".to_string(),
                    score_range: "0-5".to_string(),
                    descriptor: "Limited content with minimal ideas and little evidence of deliberate choices".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "organisation".to_string(),
            title: "Organisation".to_string(),
            description: Some("Structure, paragraphing, cohesion, discourse markers".to_string()),
            max_score: 24.0,
            weight: 0.25,
            rubric_levels: vec![
                RubricLevel {
                    level: "Band 5".to_string(),
                    score_range: "21-24".to_string(),
                    descriptor: "Highly structured with sophisticated paragraphing, integrated cohesive devices, and fluent discourse markers".to_string(),
                },
                RubricLevel {
                    level: "Band 4".to_string(),
                    score_range: "16-20".to_string(),
                    descriptor: "Well-structured with effective paragraphing and varied cohesive devices".to_string(),
                },
                RubricLevel {
                    level: "Band 3".to_string(),
                    score_range: "11-15".to_string(),
                    descriptor: "Structured with clear paragraphing and some cohesive devices used appropriately".to_string(),
                },
                RubricLevel {
                    level: "Band 2".to_string(),
                    score_range: "6-10".to_string(),
                    descriptor: "Some structural features with basic paragraphing and simple connectives".to_string(),
                },
                RubricLevel {
                    level: "Band 1".to_string(),
                    score_range: "0-5".to_string(),
                    descriptor: "Minimal structure with little or no paragraphing and few connectives".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "vocabulary_sentence_structure".to_string(),
            title: "Vocabulary & Sentence Structure".to_string(),
            description: Some("Range, effectiveness, variety".to_string()),
            max_score: 24.0,
            weight: 0.25,
            rubric_levels: vec![
                RubricLevel {
                    level: "Band 5".to_string(),
                    score_range: "21-24".to_string(),
                    descriptor: "Extensive vocabulary with ambitious, precise word choices and a full range of sentence structures used for effect".to_string(),
                },
                RubricLevel {
                    level: "Band 4".to_string(),
                    score_range: "16-20".to_string(),
                    descriptor: "Varied vocabulary with some sophisticated choices and varied sentence structures".to_string(),
                },
                RubricLevel {
                    level: "Band 3".to_string(),
                    score_range: "11-15".to_string(),
                    descriptor: "Reasonable vocabulary with some conscious choices and varied sentence structures attempted".to_string(),
                },
                RubricLevel {
                    level: "Band 2".to_string(),
                    score_range: "6-10".to_string(),
                    descriptor: "Simple vocabulary with occasional effective choices and some sentence variety".to_string(),
                },
                RubricLevel {
                    level: "Band 1".to_string(),
                    score_range: "0-5".to_string(),
                    descriptor: "Limited vocabulary with simple word choices and little sentence variety".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "technical_accuracy".to_string(),
            title: "Technical Accuracy".to_string(),
            description: Some("Spelling, punctuation, grammar".to_string()),
            max_score: 16.0,
            weight: 0.25,
            rubric_levels: vec![
                RubricLevel {
                    level: "Band 5".to_string(),
                    score_range: "14-16".to_string(),
                    descriptor: "Consistently accurate spelling, punctuation, and grammar with full control of complex constructions".to_string(),
                },
                RubricLevel {
                    level: "Band 4".to_string(),
                    score_range: "11-13".to_string(),
                    descriptor: "Generally accurate with only occasional errors in spelling, punctuation, or grammar".to_string(),
                },
                RubricLevel {
                    level: "Band 3".to_string(),
                    score_range: "7-10".to_string(),
                    descriptor: "Mostly accurate with some errors that do not significantly impede meaning".to_string(),
                },
                RubricLevel {
                    level: "Band 2".to_string(),
                    score_range: "4-6".to_string(),
                    descriptor: "Some accuracy in basic spelling, punctuation, and grammar with frequent errors".to_string(),
                },
                RubricLevel {
                    level: "Band 1".to_string(),
                    score_range: "0-3".to_string(),
                    descriptor: "Limited accuracy with persistent errors that impede meaning".to_string(),
                },
            ],
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "gcse_english".to_string(),
        name: "GCSE English Language Marking Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(40.0),
        criteria,
    }
}

/// Built-in NHS clinical governance assessment framework.
pub fn nhs_clinical_governance_framework() -> EvaluationFramework {
    let rubric = |desc_excellent: &str, desc_good: &str, desc_requires: &str, desc_inadequate: &str| -> Vec<RubricLevel> {
        vec![
            RubricLevel {
                level: "Excellent".to_string(),
                score_range: "9-10".to_string(),
                descriptor: desc_excellent.to_string(),
            },
            RubricLevel {
                level: "Good".to_string(),
                score_range: "6-8".to_string(),
                descriptor: desc_good.to_string(),
            },
            RubricLevel {
                level: "Requires Improvement".to_string(),
                score_range: "4-5".to_string(),
                descriptor: desc_requires.to_string(),
            },
            RubricLevel {
                level: "Inadequate".to_string(),
                score_range: "0-3".to_string(),
                descriptor: desc_inadequate.to_string(),
            },
        ]
    };

    let criteria = vec![
        EvaluationCriterion {
            id: "patient_safety".to_string(),
            title: "Patient Safety".to_string(),
            description: Some("Risk management, incident reporting, safeguarding".to_string()),
            max_score: 10.0,
            weight: 0.25,
            rubric_levels: rubric(
                "Proactive safety culture with robust incident reporting, systematic risk management, and exemplary safeguarding",
                "Good safety systems with effective incident reporting and risk management in place",
                "Basic safety measures but gaps in incident reporting or risk management processes",
                "Serious safety concerns with inadequate incident reporting or safeguarding failures",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "clinical_effectiveness".to_string(),
            title: "Clinical Effectiveness".to_string(),
            description: Some("Evidence-based practice, audit, outcomes".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Systematic evidence-based practice with comprehensive audit programme and excellent patient outcomes",
                "Good evidence-based practice with regular clinical audit and monitoring of outcomes",
                "Some evidence-based practice but limited audit activity or outcome monitoring",
                "Poor adherence to evidence-based practice with no meaningful audit or outcome data",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "patient_experience".to_string(),
            title: "Patient Experience".to_string(),
            description: Some("Feedback mechanisms, complaints handling, dignity".to_string()),
            max_score: 10.0,
            weight: 0.20,
            rubric_levels: rubric(
                "Outstanding patient experience with proactive feedback, responsive complaints handling, and dignity embedded in culture",
                "Good patient experience with effective feedback mechanisms and timely complaints resolution",
                "Basic patient feedback collected but complaints handling slow or dignity not consistently maintained",
                "Poor patient experience with no feedback mechanisms or unresolved complaints",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "staff_management".to_string(),
            title: "Staff Management".to_string(),
            description: Some("Training, supervision, workforce planning".to_string()),
            max_score: 10.0,
            weight: 0.15,
            rubric_levels: rubric(
                "Excellent workforce planning with comprehensive training, regular supervision, and high staff retention",
                "Good training programmes with adequate supervision and workforce planning",
                "Basic training in place but gaps in supervision or workforce planning",
                "Inadequate training, poor supervision, or critical workforce gaps",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "information_governance".to_string(),
            title: "Information Governance".to_string(),
            description: Some("Data security, confidentiality, record keeping".to_string()),
            max_score: 10.0,
            weight: 0.10,
            rubric_levels: rubric(
                "Exemplary information governance with robust data security, full confidentiality compliance, and excellent record keeping",
                "Good data security and confidentiality practices with adequate record keeping",
                "Basic information governance but vulnerabilities in data security or record keeping",
                "Serious information governance failures with data breaches or poor record keeping",
            ),
            sub_criteria: vec![],
        },
        EvaluationCriterion {
            id: "leadership_accountability".to_string(),
            title: "Leadership & Accountability".to_string(),
            description: Some("Governance structures, transparency".to_string()),
            max_score: 10.0,
            weight: 0.10,
            rubric_levels: rubric(
                "Strong governance structures with clear accountability, transparency, and effective board oversight",
                "Good governance with defined accountability and regular reporting",
                "Basic governance structures but accountability unclear or reporting inconsistent",
                "Weak governance with no clear accountability or transparency",
            ),
            sub_criteria: vec![],
        },
    ];

    EvaluationFramework {
        id: "nhs_clinical_governance".to_string(),
        name: "NHS Clinical Governance Assessment Framework".to_string(),
        total_weight: 1.0,
        pass_mark: Some(65.0),
        criteria,
    }
}

/// Select the appropriate built-in framework based on intent keywords.
pub fn framework_for_intent(intent: &str) -> EvaluationFramework {
    let lower = intent.to_lowercase();
    if lower.contains("green book") || lower.contains("impact assessment") {
        policy_greenbook_framework()
    } else if lower.contains("survey") || lower.contains("methodology") || lower.contains("questionnaire") {
        survey_methodology_framework()
    } else if lower.contains("contract") || lower.contains("legal") {
        contract_review_framework()
    } else if lower.contains("gcse") || lower.contains("english language") {
        gcse_english_framework()
    } else if lower.contains("nhs") || lower.contains("clinical") || lower.contains("governance") {
        nhs_clinical_governance_framework()
    } else if lower.contains("essay") || lower.contains("mark") || lower.contains("a-level") || lower.contains("coursework") {
        academic_essay_framework()
    } else {
        generic_quality_framework()
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

    #[test]
    fn test_policy_greenbook_weights_sum_to_1() {
        let fw = policy_greenbook_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "Policy Green Book weights should sum to 1.0, got {}",
            total
        );
        assert_eq!(fw.criteria.len(), 6);
        assert_eq!(fw.pass_mark, Some(60.0));
    }

    #[test]
    fn test_survey_methodology_weights_sum_to_1() {
        let fw = survey_methodology_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "Survey methodology weights should sum to 1.0, got {}",
            total
        );
        assert_eq!(fw.criteria.len(), 6);
        assert_eq!(fw.pass_mark, Some(50.0));
    }

    #[test]
    fn test_contract_review_weights_sum_to_1() {
        let fw = contract_review_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "Contract review weights should sum to 1.0, got {}",
            total
        );
        assert_eq!(fw.criteria.len(), 6);
        assert_eq!(fw.pass_mark, Some(60.0));
    }

    #[test]
    fn test_gcse_english_weights_sum_to_1() {
        let fw = gcse_english_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "GCSE English weights should sum to 1.0, got {}",
            total
        );
        assert_eq!(fw.criteria.len(), 4);
        assert_eq!(fw.pass_mark, Some(40.0));
    }

    #[test]
    fn test_nhs_clinical_governance_weights_sum_to_1() {
        let fw = nhs_clinical_governance_framework();
        let total: f64 = fw.criteria.iter().map(|c| c.weight).sum();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "NHS clinical governance weights should sum to 1.0, got {}",
            total
        );
        assert_eq!(fw.criteria.len(), 6);
        assert_eq!(fw.pass_mark, Some(65.0));
    }

    #[test]
    fn test_gcse_has_five_bands() {
        let fw = gcse_english_framework();
        for criterion in &fw.criteria {
            assert_eq!(
                criterion.rubric_levels.len(),
                5,
                "GCSE criterion '{}' should have 5 bands",
                criterion.title
            );
            assert_eq!(criterion.rubric_levels[0].level, "Band 5");
            assert_eq!(criterion.rubric_levels[4].level, "Band 1");
        }
    }

    #[test]
    fn test_framework_for_intent() {
        assert_eq!(framework_for_intent("evaluate this green book assessment").id, "policy_greenbook");
        assert_eq!(framework_for_intent("review this impact assessment").id, "policy_greenbook");
        assert_eq!(framework_for_intent("check this survey methodology").id, "survey_methodology");
        assert_eq!(framework_for_intent("review this questionnaire design").id, "survey_methodology");
        assert_eq!(framework_for_intent("review this contract").id, "contract_review");
        assert_eq!(framework_for_intent("assess this legal document").id, "contract_review");
        assert_eq!(framework_for_intent("mark this gcse paper").id, "gcse_english");
        assert_eq!(framework_for_intent("evaluate english language work").id, "gcse_english");
        assert_eq!(framework_for_intent("assess nhs service").id, "nhs_clinical_governance");
        assert_eq!(framework_for_intent("review clinical practice").id, "nhs_clinical_governance");
        assert_eq!(framework_for_intent("check governance framework").id, "nhs_clinical_governance");
        assert_eq!(framework_for_intent("mark this essay").id, "academic_essay");
        assert_eq!(framework_for_intent("score this tender bid").id, "generic_quality");
        assert_eq!(framework_for_intent("evaluate this random thing").id, "generic_quality");
    }
}
