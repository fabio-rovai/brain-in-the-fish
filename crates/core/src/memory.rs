//! Cross-evaluation memory store.
//!
//! Persists evaluation results to disk for cross-document comparison.
//! After evaluating N documents, the system can answer:
//! "How does this essay compare to the last 10?"

use crate::moderation::OverallResult;
use crate::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A stored evaluation summary (minimal — not the full session).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvaluationRecord {
    pub id: String,
    pub document_title: String,
    pub doc_type: String,
    pub intent: String,
    pub framework_name: String,
    pub overall_score: f64,
    pub max_possible: f64,
    pub percentage: f64,
    pub passed: Option<bool>,
    pub criterion_scores: Vec<CriterionRecord>,
    pub agent_count: usize,
    pub debate_rounds: usize,
    pub evaluated_at: String,
    /// Top weaknesses identified during evaluation.
    #[serde(default)]
    pub weaknesses: Vec<String>,
    /// Document type classification (essay, policy, contract, etc).
    #[serde(default)]
    pub document_type: String,
    /// ISO 8601 timestamp of when this record was created.
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CriterionRecord {
    pub criterion_id: String,
    pub criterion_title: String,
    pub consensus_score: f64,
    pub max_score: f64,
    pub percentage: f64,
}

/// The memory store — reads/writes to ~/.brain-in-the-fish/history/
pub struct MemoryStore {
    dir: PathBuf,
}

impl MemoryStore {
    /// Create or open the memory store.
    pub fn open() -> anyhow::Result<Self> {
        let dir = dirs_or_home().join(".brain-in-the-fish").join("history");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Returns the history directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Create or open the memory store at a custom directory.
    pub fn open_at(dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Save an evaluation record.
    pub fn save(&self, record: &EvaluationRecord) -> anyhow::Result<PathBuf> {
        let filename = format!("{}.json", record.id);
        let path = self.dir.join(&filename);
        let json = serde_json::to_string_pretty(record)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load all historical records.
    pub fn load_all(&self) -> anyhow::Result<Vec<EvaluationRecord>> {
        let mut records = Vec::new();
        if !self.dir.exists() {
            return Ok(records);
        }
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false)
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(record) = serde_json::from_str::<EvaluationRecord>(&content)
            {
                records.push(record);
            }
        }
        records.sort_by(|a, b| a.evaluated_at.cmp(&b.evaluated_at));
        Ok(records)
    }

    /// Load records matching a framework/doc_type.
    pub fn load_matching(&self, framework_name: &str) -> anyhow::Result<Vec<EvaluationRecord>> {
        let all = self.load_all()?;
        Ok(all
            .into_iter()
            .filter(|r| r.framework_name == framework_name)
            .collect())
    }

    /// Generate comparison statistics against historical records.
    pub fn compare(
        &self,
        current: &EvaluationRecord,
    ) -> anyhow::Result<Option<ComparisonResult>> {
        let matching = self.load_matching(&current.framework_name)?;
        if matching.is_empty() {
            return Ok(None);
        }

        let scores: Vec<f64> = matching.iter().map(|r| r.percentage).collect();
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let std_dev = if scores.len() > 1 {
            let variance =
                scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (scores.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        // Percentile rank
        let below = scores.iter().filter(|&&s| s < current.percentage).count();
        let percentile = (below as f64 / scores.len() as f64 * 100.0) as u32;

        // Per-criterion comparison
        let mut criterion_comparisons = Vec::new();
        for cs in &current.criterion_scores {
            let historical: Vec<f64> = matching
                .iter()
                .flat_map(|r| r.criterion_scores.iter())
                .filter(|c| c.criterion_title == cs.criterion_title)
                .map(|c| c.percentage)
                .collect();
            if !historical.is_empty() {
                let hist_mean = historical.iter().sum::<f64>() / historical.len() as f64;
                criterion_comparisons.push(CriterionComparison {
                    criterion_title: cs.criterion_title.clone(),
                    current_percentage: cs.percentage,
                    historical_mean: hist_mean,
                    delta: cs.percentage - hist_mean,
                });
            }
        }

        Ok(Some(ComparisonResult {
            total_compared: matching.len(),
            historical_mean: mean,
            historical_std_dev: std_dev,
            current_percentage: current.percentage,
            percentile,
            criterion_comparisons,
        }))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComparisonResult {
    pub total_compared: usize,
    pub historical_mean: f64,
    pub historical_std_dev: f64,
    pub current_percentage: f64,
    pub percentile: u32,
    pub criterion_comparisons: Vec<CriterionComparison>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CriterionComparison {
    pub criterion_title: String,
    pub current_percentage: f64,
    pub historical_mean: f64,
    pub delta: f64,
}

// ============================================================================
// Cross-evaluation report
// ============================================================================

/// Summary of cross-evaluation analysis, suitable for JSON serialization.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossEvaluationSummary {
    pub evaluations: usize,
    pub mean_score: f64,
    pub std_dev: f64,
    pub min_score: f64,
    pub max_score: f64,
    pub trend: String,
    pub weakest_criteria: Vec<String>,
    pub common_weaknesses: Vec<String>,
    pub by_document_type: HashMap<String, usize>,
}

/// Load all past evaluations from a history directory and produce a markdown comparison report.
pub fn cross_evaluation_report(history_dir: &Path) -> anyhow::Result<String> {
    let store = MemoryStore::open_at(history_dir.to_path_buf())?;
    let records = store.load_all()?;

    if records.is_empty() {
        return Ok("# Cross-Evaluation Report\n\nNo evaluations found in history.\n".to_string());
    }

    let summary = compute_cross_summary(&records);
    Ok(format_cross_report(&records, &summary))
}

/// Compute the cross-evaluation summary from a set of records (for JSON output).
pub fn cross_evaluation_summary(history_dir: &Path) -> anyhow::Result<CrossEvaluationSummary> {
    let store = MemoryStore::open_at(history_dir.to_path_buf())?;
    let records = store.load_all()?;

    if records.is_empty() {
        return Ok(CrossEvaluationSummary {
            evaluations: 0,
            mean_score: 0.0,
            std_dev: 0.0,
            min_score: 0.0,
            max_score: 0.0,
            trend: "no_data".to_string(),
            weakest_criteria: vec![],
            common_weaknesses: vec![],
            by_document_type: HashMap::new(),
        });
    }

    Ok(compute_cross_summary(&records))
}

fn compute_cross_summary(records: &[EvaluationRecord]) -> CrossEvaluationSummary {
    let scores: Vec<f64> = records.iter().map(|r| r.percentage).collect();
    let n = scores.len() as f64;
    let mean = scores.iter().sum::<f64>() / n;
    let std_dev = if scores.len() > 1 {
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n - 1.0);
        variance.sqrt()
    } else {
        0.0
    };
    let min_score = scores.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_score = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Trend: compare first half vs second half mean
    let trend = if scores.len() >= 4 {
        let mid = scores.len() / 2;
        let first_half_mean = scores[..mid].iter().sum::<f64>() / mid as f64;
        let second_half_mean = scores[mid..].iter().sum::<f64>() / (scores.len() - mid) as f64;
        let delta = second_half_mean - first_half_mean;
        if delta > 3.0 {
            "improving".to_string()
        } else if delta < -3.0 {
            "declining".to_string()
        } else {
            "stable".to_string()
        }
    } else {
        "insufficient_data".to_string()
    };

    // Per-criterion averages — find weakest
    let mut criterion_totals: HashMap<String, (f64, usize)> = HashMap::new();
    for record in records {
        for cs in &record.criterion_scores {
            let entry = criterion_totals
                .entry(cs.criterion_title.clone())
                .or_insert((0.0, 0));
            entry.0 += cs.percentage;
            entry.1 += 1;
        }
    }
    let mut criterion_avgs: Vec<(String, f64)> = criterion_totals
        .into_iter()
        .map(|(title, (total, count))| (title, total / count as f64))
        .collect();
    criterion_avgs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let weakest_criteria: Vec<String> = criterion_avgs
        .iter()
        .take(3)
        .map(|(title, avg)| format!("{} ({:.1}%)", title, avg))
        .collect();

    // Common weaknesses across records
    let mut weakness_counts: HashMap<String, usize> = HashMap::new();
    for record in records {
        for w in &record.weaknesses {
            *weakness_counts.entry(w.clone()).or_insert(0) += 1;
        }
    }
    let mut weakness_sorted: Vec<(String, usize)> = weakness_counts.into_iter().collect();
    weakness_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let common_weaknesses: Vec<String> = weakness_sorted
        .iter()
        .take(5)
        .map(|(w, count)| format!("{} (x{})", w, count))
        .collect();

    // By document type
    let mut by_document_type: HashMap<String, usize> = HashMap::new();
    for record in records {
        let dt = if record.document_type.is_empty() {
            &record.doc_type
        } else {
            &record.document_type
        };
        *by_document_type
            .entry(if dt.is_empty() {
                "unknown".to_string()
            } else {
                dt.clone()
            })
            .or_insert(0) += 1;
    }

    CrossEvaluationSummary {
        evaluations: records.len(),
        mean_score: mean,
        std_dev,
        min_score,
        max_score,
        trend,
        weakest_criteria,
        common_weaknesses,
        by_document_type,
    }
}

fn format_cross_report(records: &[EvaluationRecord], summary: &CrossEvaluationSummary) -> String {
    let mut report = String::new();
    report.push_str("# Cross-Evaluation Report\n\n");
    report.push_str(&format!("**Total evaluations:** {}\n\n", summary.evaluations));

    // Score distribution
    report.push_str("## Score Distribution\n\n");
    report.push_str("| Metric | Value |\n|--------|-------|\n");
    report.push_str(&format!("| Mean | {:.1}% |\n", summary.mean_score));
    report.push_str(&format!("| Std Dev | {:.1}% |\n", summary.std_dev));
    report.push_str(&format!("| Min | {:.1}% |\n", summary.min_score));
    report.push_str(&format!("| Max | {:.1}% |\n", summary.max_score));
    report.push_str(&format!("| Trend | {} |\n\n", summary.trend));

    // Percentiles
    let mut sorted_scores: Vec<f64> = records.iter().map(|r| r.percentage).collect();
    sorted_scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p25 = percentile_value(&sorted_scores, 25);
    let p50 = percentile_value(&sorted_scores, 50);
    let p75 = percentile_value(&sorted_scores, 75);
    report.push_str(&format!(
        "**Percentiles:** P25={:.1}% | P50={:.1}% | P75={:.1}%\n\n",
        p25, p50, p75
    ));

    // Weakest criteria
    if !summary.weakest_criteria.is_empty() {
        report.push_str("## Weakest Criteria (Consistently Low)\n\n");
        for c in &summary.weakest_criteria {
            report.push_str(&format!("- {}\n", c));
        }
        report.push('\n');
    }

    // Common weaknesses
    if !summary.common_weaknesses.is_empty() {
        report.push_str("## Common Weaknesses\n\n");
        for w in &summary.common_weaknesses {
            report.push_str(&format!("- {}\n", w));
        }
        report.push('\n');
    }

    // By document type
    if !summary.by_document_type.is_empty() {
        report.push_str("## By Document Type\n\n");
        for (dt, count) in &summary.by_document_type {
            report.push_str(&format!("- **{}**: {} evaluations\n", dt, count));
        }
        report.push('\n');
    }

    // Performance over time
    report.push_str("## Performance Over Time\n\n");
    report.push_str("| # | Date | Document | Score |\n|---|------|----------|-------|\n");
    for (i, record) in records.iter().enumerate() {
        let date = if record.timestamp.is_empty() {
            &record.evaluated_at
        } else {
            &record.timestamp
        };
        let short_date = if date.len() >= 10 { &date[..10] } else { date };
        let title = if record.document_title.len() > 40 {
            format!("{}...", &record.document_title[..37])
        } else {
            record.document_title.clone()
        };
        report.push_str(&format!(
            "| {} | {} | {} | {:.1}% |\n",
            i + 1,
            short_date,
            title,
            record.percentage
        ));
    }
    report.push('\n');

    report
}

fn percentile_value(sorted: &[f64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p as f64 / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Build an EvaluationRecord from session + overall result.
pub fn build_record(
    session: &EvaluationSession,
    overall: &OverallResult,
    intent: &str,
) -> EvaluationRecord {
    let criterion_scores: Vec<CriterionRecord> = session
        .final_scores
        .iter()
        .map(|ms| {
            let crit = session
                .framework
                .criteria
                .iter()
                .find(|c| c.id == ms.criterion_id);
            let title = crit
                .map(|c| c.title.clone())
                .unwrap_or_else(|| ms.criterion_id.clone());
            let max = crit.map(|c| c.max_score).unwrap_or(10.0);
            CriterionRecord {
                criterion_id: ms.criterion_id.clone(),
                criterion_title: title,
                consensus_score: ms.consensus_score,
                max_score: max,
                percentage: if max > 0.0 {
                    ms.consensus_score / max * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();

    // Identify weaknesses: criteria scoring below 50%
    let weaknesses: Vec<String> = criterion_scores
        .iter()
        .filter(|cs| cs.percentage < 50.0)
        .map(|cs| format!("{} ({:.0}%)", cs.criterion_title, cs.percentage))
        .collect();

    let now = chrono::Utc::now().to_rfc3339();

    EvaluationRecord {
        id: session.id.clone(),
        document_title: session.document.title.clone(),
        doc_type: session.document.doc_type.clone(),
        intent: intent.to_string(),
        framework_name: session.framework.name.clone(),
        overall_score: overall.total_score,
        max_possible: overall.max_possible,
        percentage: overall.percentage,
        passed: overall.passed,
        criterion_scores,
        agent_count: session.agents.len(),
        debate_rounds: session.rounds.len(),
        evaluated_at: session.created_at.clone(),
        weaknesses,
        document_type: session.document.doc_type.clone(),
        timestamp: now,
    }
}

fn dirs_or_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(id: &str, framework: &str, percentage: f64) -> EvaluationRecord {
        let weaknesses = if percentage < 50.0 {
            vec![format!("Clarity ({:.0}%)", percentage)]
        } else {
            vec![]
        };
        EvaluationRecord {
            id: id.to_string(),
            document_title: format!("Doc {id}"),
            doc_type: "essay".to_string(),
            intent: "evaluate quality".to_string(),
            framework_name: framework.to_string(),
            overall_score: percentage / 10.0,
            max_possible: 10.0,
            percentage,
            passed: Some(percentage >= 50.0),
            criterion_scores: vec![CriterionRecord {
                criterion_id: "c1".to_string(),
                criterion_title: "Clarity".to_string(),
                consensus_score: percentage / 10.0,
                max_score: 10.0,
                percentage,
            }],
            agent_count: 3,
            debate_rounds: 2,
            evaluated_at: format!("2026-01-{:02}T00:00:00Z", id.parse::<u32>().unwrap_or(1)),
            weaknesses,
            document_type: "essay".to_string(),
            timestamp: format!("2026-01-{:02}T00:00:00Z", id.parse::<u32>().unwrap_or(1)),
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open_at(dir.path().to_path_buf()).unwrap();

        let record = make_record("1", "Academic Essay", 75.0);
        let path = store.save(&record).unwrap();
        assert!(path.exists());

        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "1");
        assert!((loaded[0].percentage - 75.0).abs() < 1e-10);
    }

    #[test]
    fn test_load_matching() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open_at(dir.path().to_path_buf()).unwrap();

        store.save(&make_record("1", "Academic Essay", 70.0)).unwrap();
        store.save(&make_record("2", "Academic Essay", 80.0)).unwrap();
        store.save(&make_record("3", "Policy Review", 60.0)).unwrap();

        let matching = store.load_matching("Academic Essay").unwrap();
        assert_eq!(matching.len(), 2);

        let policy = store.load_matching("Policy Review").unwrap();
        assert_eq!(policy.len(), 1);
    }

    #[test]
    fn test_compare() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open_at(dir.path().to_path_buf()).unwrap();

        store.save(&make_record("1", "Academic Essay", 60.0)).unwrap();
        store.save(&make_record("2", "Academic Essay", 70.0)).unwrap();
        store.save(&make_record("3", "Academic Essay", 80.0)).unwrap();

        let current = make_record("4", "Academic Essay", 85.0);
        let comparison = store.compare(&current).unwrap().unwrap();

        assert_eq!(comparison.total_compared, 3);
        assert!((comparison.historical_mean - 70.0).abs() < 1e-10);
        assert!((comparison.current_percentage - 85.0).abs() < 1e-10);
        // 85 > all 3 historical scores, so percentile = 100
        assert_eq!(comparison.percentile, 100);
        assert_eq!(comparison.criterion_comparisons.len(), 1);
    }

    #[test]
    fn test_compare_no_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open_at(dir.path().to_path_buf()).unwrap();

        let current = make_record("1", "New Framework", 75.0);
        let comparison = store.compare(&current).unwrap();
        assert!(comparison.is_none());
    }

    #[test]
    fn test_cross_evaluation_report() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open_at(dir.path().to_path_buf()).unwrap();

        // Create 3 mock history files with increasing scores
        store.save(&make_record("1", "Academic Essay", 55.0)).unwrap();
        store.save(&make_record("2", "Academic Essay", 65.0)).unwrap();
        store.save(&make_record("3", "Academic Essay", 75.0)).unwrap();

        let report = cross_evaluation_report(dir.path()).unwrap();

        // Verify it produces a report with stats
        assert!(report.contains("Cross-Evaluation Report"));
        assert!(report.contains("Score Distribution"));
        assert!(report.contains("Mean"));
        assert!(report.contains("Performance Over Time"));
        assert!(report.contains("Doc 1"));
        assert!(report.contains("Doc 3"));

        // Verify summary
        let summary = cross_evaluation_summary(dir.path()).unwrap();
        assert_eq!(summary.evaluations, 3);
        assert!((summary.mean_score - 65.0).abs() < 1e-10);
        assert!((summary.min_score - 55.0).abs() < 1e-10);
        assert!((summary.max_score - 75.0).abs() < 1e-10);
        assert_eq!(summary.by_document_type.get("essay"), Some(&3));
    }

    #[test]
    fn test_cross_evaluation_report_empty() {
        let dir = tempfile::tempdir().unwrap();
        let report = cross_evaluation_report(dir.path()).unwrap();
        assert!(report.contains("No evaluations found"));
    }
}
