mod config;
mod graph;
mod node;
mod nostr_bridge;
mod output;
mod scenarios;
mod sim;
mod types;

use clap::{Parser, Subcommand};
use config::SimConfig;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "gozzip-sim", about = "Gozzip network simulator")]
struct Cli {
    /// Path to a TOML config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Override the random seed
    #[arg(long, global = true)]
    seed: Option<u64>,

    /// Override the number of nodes
    #[arg(long, global = true)]
    nodes: Option<u32>,

    /// Override BA edges per node (follow count)
    #[arg(long = "ba-edges", global = true)]
    ba_edges: Option<u32>,

    /// Run in deterministic mode
    #[arg(long, global = true)]
    deterministic: bool,

    /// Print per-tick summary lines to stderr instead of progress bar
    #[arg(long, global = true)]
    live: bool,

    /// Write JSONL streaming log to the given path
    #[arg(long, global = true)]
    jsonl: Option<PathBuf>,

    /// Generate real NIP-01 Nostr events (requires nostr-events feature)
    #[arg(long, global = true)]
    nostr_events: bool,

    /// Override graph model (barabasi-albert, watts-strogatz, lfr)
    #[arg(long = "graph-model", global = true)]
    graph_model: Option<String>,

    /// Override WS neighbors (k parameter)
    #[arg(long = "ws-neighbors", global = true)]
    ws_neighbors: Option<u32>,

    /// Override WS rewire probability
    #[arg(long = "ws-rewire", global = true)]
    ws_rewire: Option<f64>,

    /// Enable timezone-based temporal correlation
    #[arg(long, global = true)]
    timezone: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run validation formulas against the config
    Validate {
        /// Run at scale (min 1000 nodes, 7 days) with extra metrics
        #[arg(long)]
        scale: bool,
    },

    /// Run a stress scenario
    Stress {
        #[command(subcommand)]
        scenario: StressScenario,
    },
}

#[derive(Subcommand, Debug)]
enum StressScenario {
    /// Sybil attack scenario
    Sybil {
        /// Number of sybil nodes to inject
        #[arg(long, default_value_t = 1000)]
        sybils: u32,

        /// Target node ID
        #[arg(long, default_value_t = 0)]
        target: u32,
    },

    /// Viral content scenario
    Viral {
        /// Number of viewers
        #[arg(long, default_value_t = 10000)]
        viewers: u32,

        /// Time window in minutes
        #[arg(long, default_value_t = 60)]
        window_minutes: u32,
    },

    /// Network partition scenario
    Partition {
        /// Number of partitions
        #[arg(long, default_value_t = 2)]
        partitions: u32,

        /// Duration in hours
        #[arg(long, default_value_t = 6)]
        duration_hours: u32,
    },

    /// Node churn scenario
    Churn {
        /// Churn percentage per tick
        #[arg(long, default_value_t = 10.0)]
        churn_pct: f64,

        /// Duration in hours
        #[arg(long, default_value_t = 24)]
        duration_hours: u32,
    },

    /// Eclipse attack + churn storm scenario
    Eclipse {
        /// Number of sybil nodes
        #[arg(long, default_value_t = 100)]
        sybils: u32,

        /// Target node (0 = highest-degree)
        #[arg(long, default_value_t = 0)]
        target: u32,

        /// Percentage of legitimate nodes forced offline
        #[arg(long, default_value_t = 30.0)]
        churn_pct: f64,

        /// Storm start as fraction of sim duration (0.0..1.0)
        #[arg(long, default_value_t = 0.5)]
        churn_start: f64,

        /// Storm duration in hours
        #[arg(long, default_value_t = 12)]
        churn_hours: u32,
    },

    /// Karma economics scenario
    Karma {
        /// Scenario type: "baseline" or "free-rider"
        #[arg(long, default_value = "baseline")]
        scenario: String,

        /// Free rider percentage (only for free-rider scenario)
        #[arg(long, default_value_t = 20.0)]
        free_rider_pct: f64,
    },
}

fn main() {
    let cli = Cli::parse();

    // Load config
    let mut cfg = SimConfig::load_or_default(cli.config.as_deref());

    // Apply overrides
    if let Some(seed) = cli.seed {
        cfg.graph.seed = seed;
    }
    if let Some(nodes) = cli.nodes {
        cfg.graph.nodes = nodes;
    }
    if let Some(ba_edges) = cli.ba_edges {
        cfg.graph.ba_edges_per_node = ba_edges;
    }
    if cli.deterministic {
        cfg.simulation.deterministic = true;
    }
    if cli.live {
        cfg.simulation.streaming.live_ticks = true;
    }
    if let Some(ref p) = cli.jsonl {
        cfg.simulation.streaming.jsonl_path = p.to_string_lossy().into();
    }
    if cli.nostr_events {
        cfg.simulation.nostr_events = true;
    }
    if let Some(ref model) = cli.graph_model {
        cfg.graph.model = model.clone();
    }
    if let Some(ws_neighbors) = cli.ws_neighbors {
        cfg.graph.ws_neighbors = ws_neighbors;
    }
    if let Some(ws_rewire) = cli.ws_rewire {
        cfg.graph.ws_rewire_prob = ws_rewire;
    }
    if cli.timezone {
        cfg.network.timezone_correlation = true;
    }

    match cli.command {
        Command::Validate { scale } => {
            cfg.simulation.deterministic = true;

            // When --scale is set, enforce minimum nodes and duration
            if scale {
                cfg.graph.nodes = cfg.graph.nodes.max(1000);
                cfg.simulation.duration_days = cfg.simulation.duration_days.max(7);
            }

            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            let start = Instant::now();

            let result = rt.block_on(async {
                let orchestrator = sim::orchestrator::Orchestrator::new(cfg.clone());
                orchestrator.run().await
            });

            let elapsed = start.elapsed();

            let formula_results = scenarios::validate::validate_formulas(&result);
            scenarios::validate::print_results(&formula_results, &result.config);
            let read_latencies: Vec<f64> = result.metrics.read_results
                .iter()
                .map(|r| r.latency_secs)
                .collect();
            scenarios::validate::print_latency_summary(&read_latencies);
            scenarios::validate::print_read_tier_summary(&result.metrics.read_results);
            scenarios::validate::print_ttfp_summary(&result);
            scenarios::validate::print_relay_decay_curve(
                &result.metrics.read_results,
                &result.metrics.snapshots,
            );
            scenarios::validate::print_content_availability(
                &result.metrics.read_results,
                result.config.simulation.duration_days,
            );
            scenarios::validate::print_read_tier_by_feed_tier(&result.metrics.read_results);

            if scale {
                scenarios::validate::print_scale_metrics(&result);
            }

            println!("Simulation completed in {:.2}s", elapsed.as_secs_f64());

            // Write JSON report
            let report_path = format!(
                "results/validate-{}-seed{}.json",
                cfg.graph.nodes, cfg.graph.seed
            );
            let per_node =
                output::json::build_per_node_summary(&result.metrics, &result.config, &result.graph);
            let json_report = output::json::JsonReport {
                config: result.config.clone(),
                formulas: formula_results.clone(),
                per_node,
                sample_events: result.metrics.sample_events.clone(),
                activity_weights: result.activity_weights.clone(),
            };
            match output::json::write_report(&json_report, Path::new(&report_path)) {
                Ok(()) => println!("JSON report written to {}", report_path),
                Err(e) => eprintln!("Warning: failed to write JSON report: {}", e),
            }

            // Write HTML report
            let html_path = format!(
                "results/validate-{}-seed{}.html",
                cfg.graph.nodes, cfg.graph.seed
            );
            let charts = vec![
                output::html::degree_distribution_chart(&result.graph),
                output::html::bandwidth_chart(&result.metrics),
                output::html::formula_summary_chart(&formula_results),
            ];
            let summary = format!(
                "<p>Nodes: {} | Seed: {} | Duration: {} days | Elapsed: {:.2}s</p>",
                cfg.graph.nodes,
                cfg.graph.seed,
                cfg.simulation.duration_days,
                elapsed.as_secs_f64()
            );
            let title = format!(
                "Gozzip Validation: {} nodes, seed {}",
                cfg.graph.nodes, cfg.graph.seed
            );
            match output::html::generate_html(
                &title,
                &summary,
                &charts,
                Path::new(&html_path),
            ) {
                Ok(()) => println!("HTML report written to {}", html_path),
                Err(e) => eprintln!("Warning: failed to write HTML report: {}", e),
            }
        }
        Command::Stress { scenario } => {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

            match scenario {
                StressScenario::Sybil { sybils, target } => {
                    cfg.simulation.deterministic = true;
                    let params = scenarios::sybil::SybilParams { sybils, target };
                    rt.block_on(scenarios::sybil::run_sybil(cfg, params));
                }
                StressScenario::Viral {
                    viewers,
                    window_minutes,
                } => {
                    let params = scenarios::viral::ViralParams {
                        viewers,
                        window_minutes,
                    };
                    rt.block_on(scenarios::viral::run_viral(cfg, params));
                }
                StressScenario::Partition {
                    partitions,
                    duration_hours,
                } => {
                    cfg.simulation.deterministic = true;
                    let params = scenarios::partition::PartitionParams {
                        partitions,
                        duration_hours,
                    };
                    rt.block_on(scenarios::partition::run_partition(cfg, params));
                }
                StressScenario::Churn {
                    churn_pct,
                    duration_hours,
                } => {
                    let params = scenarios::churn::ChurnParams {
                        churn_pct,
                        duration_hours,
                    };
                    rt.block_on(scenarios::churn::run_churn(cfg, params));
                }
                StressScenario::Eclipse {
                    sybils,
                    target,
                    churn_pct,
                    churn_start,
                    churn_hours,
                } => {
                    cfg.simulation.deterministic = true;
                    let params = scenarios::eclipse::EclipseParams {
                        sybils,
                        target,
                        churn_pct,
                        churn_start_pct: churn_start,
                        churn_duration_hours: churn_hours,
                    };
                    rt.block_on(scenarios::eclipse::run_eclipse(cfg, params));
                }
                StressScenario::Karma {
                    scenario,
                    free_rider_pct,
                } => {
                    cfg.simulation.deterministic = true;
                    let karma_scenario = match scenario.as_str() {
                        "free-rider" => scenarios::karma::KarmaScenario::FreeRider { free_rider_pct },
                        _ => scenarios::karma::KarmaScenario::Baseline,
                    };
                    let params = scenarios::karma::KarmaParams {
                        scenario: karma_scenario,
                    };
                    rt.block_on(scenarios::karma::run_karma(cfg, params));
                }
            }
        }
    }
}
