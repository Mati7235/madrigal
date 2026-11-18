use std::collections::{HashMap, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyStats {
    pub samples: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyReport {
    pub total: LatencyStats,
    pub resolve: LatencyStats,
}

struct Pending {
    enqueued: Instant,
    resolved: Option<Instant>,
}

pub struct LatencyTracker {
    window: usize,
    pending: HashMap<u64, Pending>,
    totals_ms: VecDeque<f64>,
    resolve_ms: VecDeque<f64>,
}

impl LatencyTracker {
    pub fn new(window: usize) -> Self {
        Self {
            window: window.max(1),
            pending: HashMap::new(),
            totals_ms: VecDeque::new(),
            resolve_ms: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, id: u64) {
        self.enqueue_at(id, Instant::now());
    }

    pub fn enqueue_at(&mut self, id: u64, at: Instant) {
        self.pending.insert(
            id,
            Pending {
                enqueued: at,
                resolved: None,
            },
        );
    }

    pub fn resolved(&mut self, id: u64) {
        self.resolved_at(id, Instant::now());
    }

    pub fn resolved_at(&mut self, id: u64, at: Instant) {
        if let Some(pending) = self.pending.get_mut(&id) {
            pending.resolved = Some(at);
        }
    }

    pub fn started(&mut self, id: u64) -> Option<f64> {
        let at = Instant::now();
        self.started_at(id, at)
    }

    pub fn started_at(&mut self, id: u64, at: Instant) -> Option<f64> {
        let pending = self.pending.remove(&id)?;
        let total = millis_between(pending.enqueued, at);
        push_capped(&mut self.totals_ms, self.window, total);
        if let Some(mark) = pending.resolved {
            push_capped(&mut self.resolve_ms, self.window, millis_between(pending.enqueued, mark));
        }
        Some(total)
    }

    pub fn stats(&self) -> Option<LatencyReport> {
        let total = summarize(&self.totals_ms)?;
        let resolve = summarize(&self.resolve_ms).unwrap_or(LatencyStats {
            samples: 0,
            p50_ms: 0.0,
            p95_ms: 0.0,
        });
        Some(LatencyReport { total, resolve })
    }
}

fn millis_between(from: Instant, to: Instant) -> f64 {
    to.saturating_duration_since(from).as_secs_f64() * 1000.0
}

fn push_capped(queue: &mut VecDeque<f64>, cap: usize, value: f64) {
    if queue.len() >= cap {
        queue.pop_front();
    }
    queue.push_back(value);
}

fn summarize(values: &VecDeque<f64>) -> Option<LatencyStats> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    Some(LatencyStats {
        samples: sorted.len(),
        p50_ms: percentile(&sorted, 0.5),
        p95_ms: percentile(&sorted, 0.95),
    })
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.max(1) - 1]
}
