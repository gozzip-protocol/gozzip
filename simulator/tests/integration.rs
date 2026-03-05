use std::process::Command;

#[test]
fn test_validate_runs_without_error() {
    let output = Command::new("cargo")
        .args(["run", "--", "validate", "--nodes", "50", "--seed", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DEVELOPER_DIR", "/Library/Developer/CommandLineTools")
        .output()
        .expect("Failed to run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "Failed with stderr: {}", stderr);
    assert!(
        stdout.contains("Formula Validation"),
        "Missing header in output: {}",
        stdout
    );
}

#[test]
fn test_help_flag() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DEVELOPER_DIR", "/Library/Developer/CommandLineTools")
        .output()
        .expect("Failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gozzip-sim"));
}

#[test]
fn test_deterministic_flag_parsed() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "--deterministic",
            "validate",
            "--nodes",
            "20",
            "--seed",
            "1",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DEVELOPER_DIR", "/Library/Developer/CommandLineTools")
        .output()
        .expect("Failed to run");

    assert!(
        output.status.success(),
        "Failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_validate_scale_runs() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "validate",
            "--nodes",
            "100",
            "--seed",
            "1",
            "--scale",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DEVELOPER_DIR", "/Library/Developer/CommandLineTools")
        .output()
        .expect("Failed to run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "Failed with stderr: {}", stderr);
    assert!(
        stdout.contains("Formula Validation"),
        "Missing formula header in output: {}",
        stdout
    );
    assert!(
        stdout.contains("Scale Metrics"),
        "Missing scale metrics header in output: {}",
        stdout
    );
    assert!(
        stdout.contains("Pact Churn"),
        "Missing pact churn in output: {}",
        stdout
    );
    assert!(
        stdout.contains("Gossip Efficiency"),
        "Missing gossip efficiency in output: {}",
        stdout
    );
    assert!(
        stdout.contains("Cache Hit Rate"),
        "Missing cache hit rate in output: {}",
        stdout
    );
}

#[test]
fn test_stress_viral_runs() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "stress",
            "viral",
            "--viewers",
            "1000",
            "--window-minutes",
            "5",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DEVELOPER_DIR", "/Library/Developer/CommandLineTools")
        .output()
        .expect("Failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
