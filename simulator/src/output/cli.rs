use indicatif::{ProgressBar, ProgressStyle};

use crate::sim::metrics::TickSummary;

/// Create a progress bar for the simulation loop.
///
/// Displays elapsed time, a visual bar, position/total ticks, and ETA.
pub fn create_sim_progress(total_ticks: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_ticks);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ticks ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb
}

/// Finish the progress bar with a completion message.
pub fn finish_progress(pb: &ProgressBar) {
    pb.finish_with_message("Simulation complete");
}

/// Print a single-line per-tick summary to stderr.
pub fn print_tick_summary(s: &TickSummary, total_ticks: u64) {
    eprintln!(
        "[{:>6}/{:<6}] t={:.0}s | pub={} del={} | reads {}/{} | pacts +{}/\u{2212}{} (net {}) | gossip {}",
        s.tick + 1,
        total_ticks,
        s.time,
        s.events_published,
        s.events_delivered,
        s.reads_ok,
        s.reads_fail,
        s.pacts_formed,
        s.pacts_dropped,
        s.cum_pacts,
        s.gossip_sent,
    );
}
