use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::graph::Graph;
use crate::sim::metrics::CollectedMetrics;
use crate::types::{FormulaResult, FormulaStatus};

// ── ChartData ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ChartData {
    pub id: String,
    pub title: String,
    pub chart_type: String,
    pub data_json: String,
}

// ── Template ────────────────────────────────────────────────────────

const TEMPLATE: &str = include_str!("../../templates/report.html");

// ── generate_html ───────────────────────────────────────────────────

/// Generate an HTML report from a title, summary text, and a set of chart
/// definitions. Each chart is rendered into a `<div>` with inline Plotly.js
/// initialization code.
///
/// Creates parent directories if they do not exist.
pub fn generate_html(
    title: &str,
    summary: &str,
    charts: &[ChartData],
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut charts_html = String::new();

    for chart in charts {
        charts_html.push_str(&format!(
            r#"
    <div class="chart">
        <h2>{title}</h2>
        <div id="{id}" style="width:100%;height:400px;"></div>
        <script>
            (function() {{
                var data = {data_json};
                var layout = {{title: '{title}', autosize: true}};
                Plotly.newPlot('{id}', data, layout);
            }})();
        </script>
    </div>
"#,
            title = chart.title,
            id = chart.id,
            data_json = chart.data_json,
        ));
    }

    let html = TEMPLATE
        .replace("{{TITLE}}", title)
        .replace("{{SUMMARY}}", summary)
        .replace("{{CHARTS}}", &charts_html);

    fs::write(path, html)?;

    Ok(())
}

// ── Chart helpers ───────────────────────────────────────────────────

/// Create a log-log scatter chart of the degree distribution for the graph.
pub fn degree_distribution_chart(graph: &Graph) -> ChartData {
    // Count degree frequencies
    let mut degree_counts: HashMap<usize, usize> = HashMap::new();
    for id in 0..graph.node_count {
        let deg = graph.degree(id);
        *degree_counts.entry(deg).or_insert(0) += 1;
    }

    let mut degrees: Vec<usize> = degree_counts.keys().copied().collect();
    degrees.sort();

    let x: Vec<usize> = degrees.iter().copied().collect();
    let y: Vec<usize> = degrees.iter().map(|d| degree_counts[d]).collect();

    let data_json = serde_json::to_string(&serde_json::json!([{
        "x": x,
        "y": y,
        "type": "scatter",
        "mode": "markers",
        "marker": {"size": 5, "color": "#1f77b4"}
    }]))
    .unwrap_or_else(|_| "[]".to_string());

    ChartData {
        id: "degree-dist".to_string(),
        title: "Degree Distribution (log-log)".to_string(),
        chart_type: "scatter".to_string(),
        data_json,
    }
}

/// Create a bar chart showing bandwidth by category from collected metrics.
pub fn bandwidth_chart(metrics: &CollectedMetrics) -> ChartData {
    let mut gossip_up: u64 = 0;
    let mut gossip_down: u64 = 0;
    let mut pact_up: u64 = 0;
    let mut pact_down: u64 = 0;
    let mut challenge_up: u64 = 0;
    let mut challenge_down: u64 = 0;
    let mut cache_up: u64 = 0;
    let mut cache_down: u64 = 0;

    for node_metrics in metrics.snapshots.values() {
        let cat = &node_metrics.bandwidth.by_category;
        gossip_up += cat.gossip_up;
        gossip_down += cat.gossip_down;
        pact_up += cat.pact_up;
        pact_down += cat.pact_down;
        challenge_up += cat.challenge_up;
        challenge_down += cat.challenge_down;
        cache_up += cat.cache_up;
        cache_down += cat.cache_down;
    }

    let to_mb = |b: u64| b as f64 / (1024.0 * 1024.0);

    let categories = vec![
        "Gossip Up",
        "Gossip Down",
        "Pact Up",
        "Pact Down",
        "Challenge Up",
        "Challenge Down",
        "Cache Up",
        "Cache Down",
    ];
    let values = vec![
        to_mb(gossip_up),
        to_mb(gossip_down),
        to_mb(pact_up),
        to_mb(pact_down),
        to_mb(challenge_up),
        to_mb(challenge_down),
        to_mb(cache_up),
        to_mb(cache_down),
    ];

    let data_json = serde_json::to_string(&serde_json::json!([{
        "x": categories,
        "y": values,
        "type": "bar",
        "marker": {"color": "#2ca02c"}
    }]))
    .unwrap_or_else(|_| "[]".to_string());

    ChartData {
        id: "bandwidth".to_string(),
        title: "Bandwidth by Category (MB)".to_string(),
        chart_type: "bar".to_string(),
        data_json,
    }
}

/// Create a bar chart summarizing formula results (pass/warn/fail counts).
pub fn formula_summary_chart(results: &[FormulaResult]) -> ChartData {
    let passed = results
        .iter()
        .filter(|r| r.status == FormulaStatus::Pass)
        .count();
    let warnings = results
        .iter()
        .filter(|r| r.status == FormulaStatus::Warn)
        .count();
    let failed = results
        .iter()
        .filter(|r| r.status == FormulaStatus::Fail)
        .count();

    let data_json = serde_json::to_string(&serde_json::json!([{
        "x": ["Pass", "Warn", "Fail"],
        "y": [passed, warnings, failed],
        "type": "bar",
        "marker": {"color": ["#2ca02c", "#ff7f0e", "#d62728"]}
    }]))
    .unwrap_or_else(|_| "[]".to_string());

    ChartData {
        id: "formula-summary".to_string(),
        title: "Formula Validation Summary".to_string(),
        chart_type: "bar".to_string(),
        data_json,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_html_generation() {
        let charts = vec![
            ChartData {
                id: "test-chart".to_string(),
                title: "Test Chart".to_string(),
                chart_type: "bar".to_string(),
                data_json: r#"[{"x":[1,2,3],"y":[4,5,6],"type":"bar"}]"#.to_string(),
            },
        ];

        let dir = std::env::temp_dir().join("gozzip-test-html");
        let path = dir.join("test-report.html");

        generate_html("Test Report", "This is a test summary.", &charts, &path)
            .expect("HTML generation should succeed");

        let contents = fs::read_to_string(&path).expect("should read generated HTML");

        assert!(contents.contains("plotly"), "HTML should reference plotly");
        assert!(contents.contains("test-chart"), "HTML should contain chart div ID");
        assert!(contents.contains("Test Report"), "HTML should contain the title");
        assert!(contents.contains("This is a test summary"), "HTML should contain the summary");

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }
}
