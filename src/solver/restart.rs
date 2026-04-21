//! Glucose-style adaptive restarts.
//!
//! Tracks three exponential moving averages over the conflict stream: a
//! fast LBD EMA, a slow LBD EMA, and a fast trail-length EMA. A restart
//! fires when the recent LBD run-rate (fast) exceeds the long-term run-rate
//! (slow) by a configured ratio, unless the current trail is much longer
//! than its historical average in which case the restart is suppressed
//! because the solver is still making progress.
//!
//! Each EMA carries an Adam-style bias correction denominator so the first
//! few samples aren't dwarfed by their zero initial state.

/// EMA weight on new LBD samples in the fast window.
const LBD_FAST_ALPHA: f64 = 1.0 / 50.0;
/// EMA weight on new LBD samples in the slow window.
const LBD_SLOW_ALPHA: f64 = 1.0 / 10_000.0;
/// EMA weight on new trail-length samples.
const TRAIL_ALPHA: f64 = 1.0 / 50.0;
/// Multiplier applied to the slow LBD EMA. The fast EMA must exceed this
/// scaled value for a restart to fire.
const RESTART_RATIO: f64 = 1.25;
/// Multiplier applied to the trail EMA. If the current trail exceeds this
/// scaled value, a restart is suppressed.
const TRAIL_BLOCK: f64 = 1.4;
/// Minimum conflicts since the last restart before another may fire.
const RESTART_MIN_CONFLICTS: u64 = 50;

/// A single exponential moving average with Adam-style bias correction.
#[derive(Debug, Clone, Copy)]
struct Ema {
    value: f64,
    /// Grows from `0.0` toward `1.0`. Dividing `value` by `corr` removes
    /// the zero-init bias so early samples aren't underweighted.
    corr: f64,
    alpha: f64,
}

impl Ema {
    const fn new(alpha: f64) -> Self {
        Self { value: 0.0, corr: 0.0, alpha }
    }

    fn update(&mut self, x: f64) {
        self.value = self.value * (1.0 - self.alpha) + self.alpha * x;
        self.corr = self.corr * (1.0 - self.alpha) + self.alpha;
    }

    fn corrected(self) -> f64 {
        if self.corr == 0.0 {
            0.0
        } else {
            self.value / self.corr
        }
    }
}

/// Aggregate state driving the Glucose restart heuristic.
#[derive(Debug, Clone)]
pub(crate) struct RestartState {
    lbd_fast: Ema,
    lbd_slow: Ema,
    trail_fast: Ema,
    conflicts_since_restart: u64,
}

impl RestartState {
    /// Creates a fresh restart state with every EMA at zero.
    pub(crate) const fn new() -> Self {
        Self {
            lbd_fast: Ema::new(LBD_FAST_ALPHA),
            lbd_slow: Ema::new(LBD_SLOW_ALPHA),
            trail_fast: Ema::new(TRAIL_ALPHA),
            conflicts_since_restart: 0,
        }
    }

    /// Records one conflict: feeds `lbd` into the two LBD EMAs and
    /// `trail_len` into the trail EMA, and increments the conflicts-since-
    /// restart counter.
    pub(crate) fn record_conflict(&mut self, lbd: f64, trail_len: f64) {
        self.lbd_fast.update(lbd);
        self.lbd_slow.update(lbd);
        self.trail_fast.update(trail_len);
        self.conflicts_since_restart = self.conflicts_since_restart.saturating_add(1);
    }

    /// Returns `true` if a restart should fire at the current conflict.
    ///
    /// The guard conditions, in order:
    /// 1. At least `RESTART_MIN_CONFLICTS` conflicts have accumulated since
    ///    the last restart.
    /// 2. The current trail does not exceed the trail EMA by more than
    ///    `TRAIL_BLOCK` (i.e. we're not in a productive streak).
    /// 3. The fast LBD EMA, scaled by `RESTART_RATIO`, exceeds the slow
    ///    LBD EMA.
    pub(crate) fn should_restart(&self, current_trail_len: f64) -> bool {
        if self.conflicts_since_restart < RESTART_MIN_CONFLICTS {
            return false;
        }
        let trail_ema = self.trail_fast.corrected();
        if trail_ema > 0.0 && current_trail_len > trail_ema * TRAIL_BLOCK {
            return false;
        }
        let fast = self.lbd_fast.corrected();
        let slow = self.lbd_slow.corrected();
        fast * RESTART_RATIO > slow
    }

    /// Clears the restart window counter after a restart fires.
    pub(crate) const fn reset_window(&mut self) {
        self.conflicts_since_restart = 0;
    }
}

impl Default for RestartState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_does_not_restart() {
        let s = RestartState::new();
        assert!(!s.should_restart(0.0));
    }

    #[test]
    fn restart_blocked_under_minimum_conflicts() {
        let mut s = RestartState::new();
        for _ in 0..10 {
            s.record_conflict(10.0, 5.0);
        }
        assert!(!s.should_restart(5.0));
    }

    #[test]
    fn improving_streak_does_not_restart() {
        // Slow EMA gets primed with high LBDs; then we feed low LBDs.
        // Fast drops below slow faster than slow can decay, so the
        // `fast * 1.25 > slow` gate stays closed.
        let mut s = RestartState::new();
        for _ in 0..1_000 {
            s.record_conflict(20.0, 10.0);
        }
        for _ in 0..100 {
            s.record_conflict(2.0, 10.0);
        }
        assert!(!s.should_restart(10.0));
    }

    #[test]
    fn lbd_spike_triggers_restart() {
        let mut s = RestartState::new();
        // Seed the slow EMA with plenty of low-LBD samples.
        for _ in 0..1_000 {
            s.record_conflict(2.0, 10.0);
        }
        // Then a burst of high-LBD samples. The fast EMA reacts quickly,
        // the slow one barely moves.
        for _ in 0..100 {
            s.record_conflict(20.0, 10.0);
        }
        assert!(s.should_restart(10.0));
    }

    #[test]
    fn long_trail_blocks_restart() {
        let mut s = RestartState::new();
        for _ in 0..1_000 {
            s.record_conflict(2.0, 10.0);
        }
        for _ in 0..100 {
            s.record_conflict(20.0, 10.0);
        }
        // At this point a restart would fire on a normal-length trail.
        assert!(s.should_restart(10.0));
        // But a trail well above the EMA blocks it.
        assert!(!s.should_restart(100.0));
    }

    #[test]
    fn reset_window_requires_recharge() {
        let mut s = RestartState::new();
        for _ in 0..1_000 {
            s.record_conflict(2.0, 10.0);
        }
        for _ in 0..100 {
            s.record_conflict(20.0, 10.0);
        }
        assert!(s.should_restart(10.0));
        s.reset_window();
        // Immediately after a reset, the minimum-conflicts gate blocks us.
        assert!(!s.should_restart(10.0));
    }

    #[test]
    fn ema_bias_corrected_tracks_constant_input() {
        let mut e = Ema::new(0.1);
        for _ in 0..500 {
            e.update(7.0);
        }
        let c = e.corrected();
        assert!((c - 7.0).abs() < 1e-6, "got {c}");
    }
}
