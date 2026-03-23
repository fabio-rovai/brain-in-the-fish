//! Cross-evaluation memory store.
//!
//! Persists evaluation results to disk for cross-document comparison.
//! After evaluating N documents, the system can answer:
//! "How does this essay compare to the last 10?"

use crate::moderation::OverallResult;
use crate::types::*;
use std::path::PathBuf;

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

    /// Create or open the memory store at a custom directory.
    #[cfg(test)]
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

/// Build an EvaluationRecord from session + overall result.
pub fn build_record(
    session: &EvaluationSession,
    overall: &OverallResult,
    intent: &str,
) -> EvaluationRecord {
    let criterion_scores = session
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
}
