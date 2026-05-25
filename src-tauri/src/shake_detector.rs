use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct ShakeConfig {
    pub reversal_threshold: usize,
    pub window: Duration,
    pub min_displacement: i32,
    pub cooldown: Duration,
}

impl Default for ShakeConfig {
    fn default() -> Self {
        Self {
            reversal_threshold: 3,
            window: Duration::from_millis(600),
            min_displacement: 30,
            cooldown: Duration::from_millis(2000),
        }
    }
}

pub struct ShakeDetector {
    history: VecDeque<(i32, i32, Instant)>,
    pub config: ShakeConfig,
    last_trigger: Option<Instant>,
}

impl ShakeDetector {
    pub fn new(config: ShakeConfig) -> Self {
        Self {
            history: VecDeque::new(),
            config,
            last_trigger: None,
        }
    }

    pub fn process(&mut self, x: i32, y: i32) -> bool {
        let now = Instant::now();

        // Enforce cooldown
        if let Some(last) = self.last_trigger {
            if now.duration_since(last) < self.config.cooldown {
                self.history.push_back((x, y, now));
                // Trim old entries
                while self.history.front().map_or(false, |&(_, _, t)| {
                    now.duration_since(t) > self.config.window
                }) {
                    self.history.pop_front();
                }
                return false;
            }
        }

        self.history.push_back((x, y, now));

        // Trim entries outside the rolling window
        while self.history.front().map_or(false, |&(_, _, t)| {
            now.duration_since(t) > self.config.window
        }) {
            self.history.pop_front();
        }

        if self.history.len() < 3 {
            return false;
        }

        // Count direction reversals along x-axis
        let reversals = self.count_reversals();

        if reversals >= self.config.reversal_threshold {
            self.last_trigger = Some(now);
            self.history.clear();
            true
        } else {
            false
        }
    }

    fn count_reversals(&self) -> usize {
        // Build directional segments: group consecutive same-direction moves >= min_displacement
        let points: Vec<i32> = self.history.iter().map(|&(x, _, _)| x).collect();

        let mut segments: Vec<i32> = Vec::new(); // net displacement per segment
        let mut seg_start = points[0];
        let mut seg_dir: Option<i32> = None; // +1 right, -1 left

        for i in 1..points.len() {
            let delta = points[i] - points[i - 1];
            if delta == 0 {
                continue;
            }
            let dir = if delta > 0 { 1 } else { -1 };

            match seg_dir {
                None => {
                    seg_dir = Some(dir);
                }
                Some(d) if d == dir => {
                    // same direction, continue segment
                }
                Some(_) => {
                    // direction changed — close previous segment
                    let displacement = (points[i - 1] - seg_start).abs();
                    if displacement >= self.config.min_displacement {
                        segments.push(displacement);
                    }
                    seg_start = points[i - 1];
                    seg_dir = Some(dir);
                }
            }
        }
        // Close last segment
        if let Some(_) = seg_dir {
            let displacement = (points[points.len() - 1] - seg_start).abs();
            if displacement >= self.config.min_displacement {
                segments.push(displacement);
            }
        }

        // Number of reversals = number of segments - 1 (each segment boundary is a reversal)
        if segments.len() > 1 {
            segments.len() - 1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detector() -> ShakeDetector {
        ShakeDetector::new(ShakeConfig {
            reversal_threshold: 3,
            window: Duration::from_millis(600),
            min_displacement: 30,
            cooldown: Duration::from_secs(100), // long cooldown so tests don't interfere
        })
    }

    #[test]
    fn test_no_shake_straight_movement() {
        let mut d = make_detector();
        for x in 0..200 {
            assert!(!d.process(x * 2, 0));
        }
    }

    #[test]
    fn test_shake_detected() {
        let mut d = make_detector();
        // Simulate 4 reversals: right 50, left 50, right 50, left 50
        let pattern = [0, 50, 0, 50, 0, 50, 0];
        let mut triggered = false;
        for &x in &pattern {
            if d.process(x, 0) {
                triggered = true;
            }
        }
        assert!(triggered);
    }
}
