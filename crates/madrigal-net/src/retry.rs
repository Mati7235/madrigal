use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter_ratio: f32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: 4,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(5),
            jitter_ratio: 0.25,
        }
    }
}

impl RetryPolicy {
    pub fn new(
        max_attempts: u32,
        base_delay: Duration,
        max_delay: Duration,
        jitter_ratio: f32,
    ) -> Self {
        RetryPolicy {
            max_attempts: max_attempts.max(1),
            base_delay,
            max_delay,
            jitter_ratio: jitter_ratio.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug)]
pub struct RetryError<E> {
    pub attempts: u32,
    pub source: E,
}

impl<E: std::fmt::Display> std::fmt::Display for RetryError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retry budget exhausted after {} attempts: {}", self.attempts, self.source)
    }
}

impl<E: std::error::Error + 'static> std::error::Error for RetryError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub fn delay_for_attempt(policy: &RetryPolicy, attempt: u32) -> Duration {
    let grown = policy.base_delay.as_secs_f64() * 2f64.powi(attempt.min(20) as i32);
    let capped = grown.min(policy.max_delay.as_secs_f64());
    let span = capped * f64::from(policy.jitter_ratio);
    let roll = rand::random::<f64>() * 2.0 - 1.0;
    Duration::from_secs_f64((capped + roll * span).max(0.0)).min(policy.max_delay)
}

#[derive(Debug, Default)]
pub struct SectionBudget {
    attempts: HashMap<String, u32>,
}

impl SectionBudget {
    pub fn new() -> Self {
        SectionBudget::default()
    }

    pub fn attempts(&self, section: &str) -> u32 {
        self.attempts.get(section).copied().unwrap_or(0)
    }

    pub fn run<T, E, F>(
        &mut self,
        policy: &RetryPolicy,
        section: &str,
        mut operation: F,
    ) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>,
    {
        let mut used = 0;
        loop {
            used += 1;
            self.attempts.insert(section.to_owned(), used);
            match operation() {
                Ok(value) => {
                    self.attempts.remove(section);
                    return Ok(value);
                }
                Err(source) => {
                    if used >= policy.max_attempts {
                        return Err(RetryError { attempts: used, source });
                    }
                    std::thread::sleep(delay_for_attempt(policy, used - 1));
                }
            }
        }
    }
}
