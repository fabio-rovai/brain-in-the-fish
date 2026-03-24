//! End-to-end integration test for the Brain in the Fish MCP server.
//!
//! Exercises the full eval_* pipeline via direct method calls on EvalServer,
//! without requiring network transport.

use brain_in_the_fish_core::server::EvalServer;

#[tokio::test]
async fn test_mcp_pipeline_end_to_end() {
    let server = EvalServer::new();

    // ── 1. Status ──────────────────────────────────────────────────────────
    let status = server.test_status();
    let status_json: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status_json["status"], "ok");
    assert!(
        status_json["version"].as_str().is_some(),
        "Status should contain version: {status}"
    );
    assert_eq!(status_json["has_document"], false);
    assert_eq!(status_json["has_framework"], false);

    // ── 2. Ingest ──────────────────────────────────────────────────────────
    let test_doc = std::env::temp_dir().join("mcp_e2e_test_doc.txt");
    std::fs::write(
        &test_doc,
        "1. Introduction\n\n\
         This essay argues that quantitative easing was effective. \
         According to Joyce et al. (2012), QE reduced gilt yields by 100 basis points. \
         The Bank of England purchased 895 billion in assets between 2009 and 2021.\n\n\
         2. Analysis\n\n\
         However, critics such as Summers (2015) argue that the distributional effects \
         were regressive, primarily benefiting asset holders. The evidence suggests that \
         while QE achieved its primary monetary policy objectives, the secondary effects \
         on inequality were significant and largely unaddressed.\n\n\
         3. Conclusion\n\n\
         In conclusion, quantitative easing was a necessary but imperfect tool. \
         Future policy should incorporate distributional impact assessments.",
    )
    .unwrap();

    let ingest_result = server.test_ingest(test_doc.to_str().unwrap(), "mark this essay").await;
    let ingest_json: serde_json::Value = serde_json::from_str(&ingest_result).unwrap();
    assert_eq!(
        ingest_json["ok"], true,
        "Ingest should succeed: {ingest_result}"
    );
    assert!(
        ingest_json["sections"].as_u64().unwrap() > 0,
        "Ingest should find sections: {ingest_result}"
    );
    assert!(
        ingest_json["triples_loaded"].as_u64().unwrap() > 0,
        "Ingest should load triples: {ingest_result}"
    );

    // ── 3. Criteria ────────────────────────────────────────────────────────
    let criteria_result = server
        .test_criteria(Some("academic"), None)
        .await;
    let criteria_json: serde_json::Value = serde_json::from_str(&criteria_result).unwrap();
    assert_eq!(
        criteria_json["ok"], true,
        "Criteria should succeed: {criteria_result}"
    );
    assert!(
        criteria_json["criteria_count"].as_u64().unwrap() > 0,
        "Framework should have criteria: {criteria_result}"
    );

    // ── 4. Align ───────────────────────────────────────────────────────────
    let align_result = server.test_align().await;
    let align_json: serde_json::Value = serde_json::from_str(&align_result).unwrap();
    assert_eq!(
        align_json["ok"], true,
        "Align should succeed: {align_result}"
    );
    // Alignments + gaps should account for all criteria
    let alignments = align_json["alignments"].as_u64().unwrap();
    let gaps = align_json["gaps"].as_u64().unwrap();
    assert!(
        alignments + gaps > 0,
        "Should have alignments or gaps: {align_result}"
    );

    // ── 5. Spawn ───────────────────────────────────────────────────────────
    let spawn_result = server.test_spawn("mark this essay").await;
    let spawn_json: serde_json::Value = serde_json::from_str(&spawn_result).unwrap();
    assert_eq!(
        spawn_json["ok"], true,
        "Spawn should succeed: {spawn_result}"
    );
    let agent_count = spawn_json["agent_count"].as_u64().unwrap();
    assert!(
        agent_count > 0,
        "Should spawn at least one agent: {spawn_result}"
    );

    // Extract agent IDs and criterion IDs for scoring
    let agents = spawn_json["agents"].as_array().unwrap();
    let criteria = criteria_json["criteria"].as_array().unwrap();

    // ── 6. Scoring tasks ───────────────────────────────────────────────────
    let tasks_result = server.test_scoring_tasks().await;
    let tasks_json: serde_json::Value = serde_json::from_str(&tasks_result).unwrap();
    assert_eq!(
        tasks_json["ok"], true,
        "Scoring tasks should succeed: {tasks_result}"
    );
    assert!(
        tasks_json["task_count"].as_u64().unwrap() > 0,
        "Should have scoring tasks: {tasks_result}"
    );

    // ── 7. Record scores (simulate agent scoring) ──────────────────────────
    // Record a score for every (agent, criterion) pair so the report can run
    for agent in agents {
        let aid = agent["id"].as_str().unwrap();
        for criterion in criteria {
            let cid = criterion["id"].as_str().unwrap();
            let max_score = criterion["max_score"].as_f64().unwrap_or(10.0);
            let score = max_score * 0.7; // simulate a 70% score
            let record_result = server
                .test_record_score(aid, cid, score, max_score, 1, "Test justification: adequate evidence provided")
                .await;
            let record_json: serde_json::Value = serde_json::from_str(&record_result).unwrap();
            assert_eq!(
                record_json["ok"], true,
                "Record score should succeed for agent {aid}, criterion {cid}: {record_result}"
            );
        }
    }

    // ── 8. Debate status ───────────────────────────────────────────────────
    let debate_result = server.test_debate_status().await;
    let debate_json: serde_json::Value = serde_json::from_str(&debate_result).unwrap();
    assert_eq!(
        debate_json["round"], 1,
        "Should be on round 1: {debate_result}"
    );
    assert!(
        debate_json["scores_recorded"].as_u64().unwrap() > 0,
        "Should have recorded scores: {debate_result}"
    );

    // ── 9. Report ──────────────────────────────────────────────────────────
    let report_result = server.test_report().await;
    let report_json: serde_json::Value = serde_json::from_str(&report_result).unwrap();
    assert_eq!(
        report_json["ok"], true,
        "Report should succeed: {report_result}"
    );
    assert!(
        report_json["overall_score"].as_f64().is_some(),
        "Report should have overall_score: {report_result}"
    );
    assert!(
        report_json["percentage"].as_f64().unwrap() > 0.0,
        "Report should have a positive percentage: {report_result}"
    );
    let report_md = report_json["report_markdown"].as_str().unwrap();
    assert!(
        report_md.contains("score") || report_md.contains("Score") || report_md.contains("Evaluation"),
        "Report markdown should contain evaluation content: {report_md}"
    );

    // ── 10. Verify status reflects completed pipeline ──────────────────────
    let final_status = server.test_status();
    let final_json: serde_json::Value = serde_json::from_str(&final_status).unwrap();
    assert_eq!(final_json["has_document"], true);
    assert_eq!(final_json["has_framework"], true);
    assert!(final_json["agent_count"].as_u64().unwrap() > 0);
    assert!(final_json["triples_loaded"].as_u64().unwrap() > 0);

    // Cleanup
    let _ = std::fs::remove_file(test_doc);
}

#[tokio::test]
async fn test_error_handling_without_prerequisites() {
    let server = EvalServer::new();

    // Align without document should fail gracefully
    let align_result = server.test_align().await;
    assert!(
        align_result.contains("error"),
        "Align without ingest should return error: {align_result}"
    );

    // Spawn without framework should fail gracefully
    let spawn_result = server.test_spawn("test").await;
    assert!(
        spawn_result.contains("error"),
        "Spawn without criteria should return error: {spawn_result}"
    );

    // Report without anything should fail gracefully
    let report_result = server.test_report().await;
    assert!(
        report_result.contains("error"),
        "Report without prerequisites should return error: {report_result}"
    );

    // Scoring tasks without anything should fail gracefully
    let tasks_result = server.test_scoring_tasks().await;
    assert!(
        tasks_result.contains("error"),
        "Scoring tasks without prerequisites should return error: {tasks_result}"
    );
}
