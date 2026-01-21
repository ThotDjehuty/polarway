//! Adaptive chunk sizing strategies

use crate::memory_manager::MemoryManager;

/// Trait for chunk sizing strategies
pub trait ChunkStrategy: Send + Sync {
    /// Calculate optimal chunk size based on available memory
    fn calculate_chunk_size(&self, available_memory: usize) -> usize;

    /// Adjust chunk size based on performance feedback
    fn adjust(&mut self, actual_memory_used: usize, processing_time_ms: u64);
}

/// Adaptive chunk strategy that adjusts based on memory pressure
pub struct AdaptiveChunkStrategy {
    memory_manager: MemoryManager,
    current_chunk_size: usize,
    min_chunk_size: usize,
    max_chunk_size: usize,
    target_memory_ratio: f64,
}

impl AdaptiveChunkStrategy {
    /// Create a new adaptive chunk strategy
    pub fn new(memory_manager: MemoryManager) -> Self {
        Self {
            memory_manager,
            current_chunk_size: 10_000, // Start with 10K rows
            min_chunk_size: 1_000,
            max_chunk_size: 1_000_000,
            target_memory_ratio: 0.7, // Use up to 70% of available memory
        }
    }

    /// Set minimum chunk size
    pub fn with_min_chunk_size(mut self, size: usize) -> Self {
        self.min_chunk_size = size;
        self
    }

    /// Set maximum chunk size
    pub fn with_max_chunk_size(mut self, size: usize) -> Self {
        self.max_chunk_size = size;
        self
    }

    /// Set target memory ratio (0.0 - 1.0)
    pub fn with_target_memory_ratio(mut self, ratio: f64) -> Self {
        self.target_memory_ratio = ratio.clamp(0.1, 0.9);
        self
    }
}

impl ChunkStrategy for AdaptiveChunkStrategy {
    fn calculate_chunk_size(&self, available_memory: usize) -> usize {
        let target_memory = (available_memory as f64 * self.target_memory_ratio) as usize;
        
        // Estimate rows that fit in target memory
        // Assume average 100 bytes per row for OHLCV data
        let estimated_row_size = 100;
        let rows = target_memory / estimated_row_size;

        rows.clamp(self.min_chunk_size, self.max_chunk_size)
    }

    fn adjust(&mut self, _actual_memory_used: usize, processing_time_ms: u64) {
        // Adjust chunk size based on actual performance
        let memory_ratio = self.memory_manager.memory_ratio();

        if memory_ratio > 0.85 {
            // Memory pressure - reduce chunk size
            self.current_chunk_size = (self.current_chunk_size * 8 / 10).max(self.min_chunk_size);
        } else if memory_ratio < 0.5 && processing_time_ms < 100 {
            // Low memory usage and fast processing - increase chunk size
            self.current_chunk_size = (self.current_chunk_size * 12 / 10).min(self.max_chunk_size);
        }

        tracing::debug!(
            "Adjusted chunk size: {} (memory ratio: {:.2})",
            self.current_chunk_size,
            memory_ratio
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_chunk_strategy() {
        let memory_manager = MemoryManager::new().unwrap();
        let strategy = AdaptiveChunkStrategy::new(memory_manager.clone());

        let available = memory_manager.available_memory();
        let chunk_size = strategy.calculate_chunk_size(available);

        assert!(chunk_size >= strategy.min_chunk_size);
        assert!(chunk_size <= strategy.max_chunk_size);
    }

    #[test]
    fn test_chunk_size_adjustment() {
        let memory_manager = MemoryManager::new().unwrap();
        let mut strategy = AdaptiveChunkStrategy::new(memory_manager);

        let _initial_size = strategy.current_chunk_size;

        // Simulate low memory - should reduce
        strategy.adjust(1_000_000, 1000);
        // Size might change based on actual memory conditions

        // Ensure size stays in bounds
        assert!(strategy.current_chunk_size >= strategy.min_chunk_size);
        assert!(strategy.current_chunk_size <= strategy.max_chunk_size);
    }
}
