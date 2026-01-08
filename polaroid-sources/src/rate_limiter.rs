//! Rate limiting for data sources

use crate::error::{Result, SourceError};
use governor::{Quota, RateLimiter as GovernorRateLimiter, clock, state::{InMemoryState, NotKeyed}};
use std::num::NonZeroU32;
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Requests per second
    pub requests_per_second: u32,
    /// Burst size (max concurrent requests)
    pub burst_size: u32,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst_size: 10,
        }
    }
}

pub struct RateLimiter {
    limiter: Arc<GovernorRateLimiter<NotKeyed, InMemoryState, clock::DefaultClock>>,
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Result<Self> {
        let rps = NonZeroU32::new(config.requests_per_second).ok_or_else(|| {
            SourceError::ConfigError("requests_per_second must be > 0".to_string())
        })?;

        let burst = NonZeroU32::new(config.burst_size).ok_or_else(|| {
            SourceError::ConfigError("burst_size must be > 0".to_string())
        })?;

        let quota = Quota::per_second(rps).allow_burst(burst);
        let limiter = GovernorRateLimiter::direct(quota);

        Ok(Self {
            limiter: Arc::new(limiter),
        })
    }

    /// Wait until rate limit allows the request
    pub async fn acquire(&self) -> Result<()> {
        loop {
            match self.limiter.check() {
                Ok(_) => {
                    debug!("Rate limit check passed");
                    return Ok(());
                }
                Err(not_until) => {
                    let wait_duration = not_until.wait_time_from(governor::clock::Clock::now(&clock::DefaultClock::default()));
                    debug!("Rate limited - waiting {:?}", wait_duration);
                    tokio::time::sleep(wait_duration).await;
                }
            }
        }
    }

    /// Try to acquire without waiting
    pub fn try_acquire(&self) -> Result<()> {
        self.limiter.check().map_err(|_| {
            SourceError::RateLimitExceeded("Rate limit exceeded".to_string())
        })?;
        Ok(())
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self {
            limiter: self.limiter.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimiterConfig {
            requests_per_second: 10,
            burst_size: 5,
        };

        let limiter = RateLimiter::new(config).unwrap();

        // First 5 requests should pass immediately (burst)
        for i in 0..5 {
            assert!(limiter.try_acquire().is_ok(), "Request {} should succeed", i);
        }

        // 6th request should fail (rate limited)
        assert!(limiter.try_acquire().is_err());

        // Wait and try again
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(limiter.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let config = RateLimiterConfig {
            requests_per_second: 100,
            burst_size: 10,
        };

        let limiter = RateLimiter::new(config).unwrap();

        // Should complete quickly
        let start = std::time::Instant::now();
        for _ in 0..10 {
            limiter.acquire().await.unwrap();
        }
        let elapsed = start.elapsed();

        // Should take less than 200ms for 10 requests at 100 req/s
        assert!(elapsed < Duration::from_millis(200));
    }
}
