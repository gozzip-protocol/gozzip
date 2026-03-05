/// Karma accounting for nodes.
///
/// Each node earns karma for storing data for others and spends karma
/// when publishing events that pact partners must store.

// ── KarmaState ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KarmaState {
    pub balance: f64,
    pub earned_total: f64,
    pub spent_total: f64,
}

impl KarmaState {
    pub fn new(initial_balance: f64) -> Self {
        Self {
            balance: initial_balance,
            earned_total: 0.0,
            spent_total: 0.0,
        }
    }

    /// Earn karma (e.g. for storing data for a pact partner).
    pub fn earn(&mut self, amount: f64) {
        self.balance += amount;
        self.earned_total += amount;
    }

    /// Spend karma (e.g. for publishing events). Returns false if insufficient balance.
    pub fn spend(&mut self, amount: f64) -> bool {
        if self.balance >= amount {
            self.balance -= amount;
            self.spent_total += amount;
            true
        } else {
            false
        }
    }
}

// ── karma_gini ─────────────────────────────────────────────────────

/// Compute the Gini coefficient for a set of karma balances.
/// Returns 0.0 for perfect equality, approaches 1.0 for maximum inequality.
pub fn karma_gini(balances: &[f64]) -> f64 {
    let n = balances.len();
    if n <= 1 {
        return 0.0;
    }

    let mut sorted: Vec<f64> = balances.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let total: f64 = sorted.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut numerator = 0.0;
    for (i, &val) in sorted.iter().enumerate() {
        numerator += (2.0 * (i + 1) as f64 - n as f64 - 1.0) * val;
    }

    numerator / (n as f64 * total)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_karma_state_new() {
        let ks = KarmaState::new(100.0);
        assert!((ks.balance - 100.0).abs() < f64::EPSILON);
        assert!((ks.earned_total - 0.0).abs() < f64::EPSILON);
        assert!((ks.spent_total - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_karma_earn() {
        let mut ks = KarmaState::new(100.0);
        ks.earn(50.0);
        assert!((ks.balance - 150.0).abs() < f64::EPSILON);
        assert!((ks.earned_total - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_karma_spend_success() {
        let mut ks = KarmaState::new(100.0);
        assert!(ks.spend(40.0));
        assert!((ks.balance - 60.0).abs() < f64::EPSILON);
        assert!((ks.spent_total - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_karma_spend_insufficient() {
        let mut ks = KarmaState::new(10.0);
        assert!(!ks.spend(50.0));
        assert!((ks.balance - 10.0).abs() < f64::EPSILON);
        assert!((ks.spent_total - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gini_perfect_equality() {
        let balances = vec![100.0, 100.0, 100.0, 100.0];
        assert!((karma_gini(&balances) - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_gini_high_inequality() {
        // One person has everything
        let balances = vec![0.0, 0.0, 0.0, 1000.0];
        let gini = karma_gini(&balances);
        assert!(gini > 0.7, "gini should be high for extreme inequality, got {}", gini);
    }

    #[test]
    fn test_gini_empty() {
        assert!((karma_gini(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gini_single() {
        assert!((karma_gini(&[50.0]) - 0.0).abs() < f64::EPSILON);
    }
}
