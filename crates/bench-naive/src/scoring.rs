//! Scoring — prompt generation and score bookkeeping.

use crate::types::{Agent, Criterion, Score, Section};

/// Generate a scoring prompt for an agent evaluating a criterion.
pub fn generate_scoring_prompt(
    agent: &Agent,
    criterion: &Criterion,
    sections: &[(&Section, f64)],
    round: u32,
) -> String {
    let section_summaries: String = sections
        .iter()
        .map(|(sec, conf)| {
            let preview: String = sec.text.chars().take(200).collect();
            format!(
                "- [{:.0}% match] {}: {}...",
                conf * 100.0,
                sec.title,
                preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let rubric_desc = criterion
        .rubric_levels
        .iter()
        .map(|r| format!("  {} ({}): {}", r.level, r.score_range, r.descriptor))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are {name} ({role}, {domain} domain, {exp}+ years experience).\n\
         \n\
         ## Round {round} Evaluation\n\
         \n\
         **Criterion:** {title}\n\
         **Description:** {desc}\n\
         **Max score:** {max}\n\
         **Weight:** {weight:.0}%\n\
         \n\
         ### Rubric\n\
         {rubric}\n\
         \n\
         ### Relevant document sections\n\
         {sections}\n\
         \n\
         Score this criterion on a scale of 0-{max}. Provide:\n\
         1. Your score (numeric)\n\
         2. Justification with specific evidence references\n\
         3. Any gaps identified\n",
        name = agent.name,
        role = agent.role,
        domain = agent.domain,
        exp = agent.years_experience.unwrap_or(5),
        title = criterion.title,
        desc = criterion.description.as_deref().unwrap_or(""),
        max = criterion.max_score,
        weight = criterion.weight * 100.0,
        rubric = if rubric_desc.is_empty() {
            "  (no rubric levels defined)".to_string()
        } else {
            rubric_desc
        },
        sections = if section_summaries.is_empty() {
            "  (no matching sections found)".to_string()
        } else {
            section_summaries
        },
    )
}

/// Record a score into the score list.
pub fn record_score(scores: &mut Vec<Score>, score: Score) {
    scores.push(score);
}

/// Get all scores for a specific round.
pub fn get_scores_for_round(scores: &[Score], round: u32) -> Vec<&Score> {
    scores.iter().filter(|s| s.round == round).collect()
}
