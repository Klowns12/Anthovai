//! Splitting a document's chunks into provider-sized batches.

/// One batch of inputs, plus where they sat in the original list so results can
/// be put back in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchPlan {
    pub start: usize,
    pub end: usize,
}

impl BatchPlan {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Contiguous batches of at most `batch_size` items.
pub fn plan_batches(total: usize, batch_size: usize) -> Vec<BatchPlan> {
    if total == 0 || batch_size == 0 {
        return Vec::new();
    }
    (0..total)
        .step_by(batch_size)
        .map(|start| BatchPlan {
            start,
            end: (start + batch_size).min(total),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_evenly_when_it_divides() {
        let plans = plan_batches(128, 64);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0], BatchPlan { start: 0, end: 64 });
        assert_eq!(
            plans[1],
            BatchPlan {
                start: 64,
                end: 128
            }
        );
    }

    #[test]
    fn the_last_batch_holds_the_remainder() {
        let plans = plan_batches(70, 64);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[1].len(), 6);
    }

    #[test]
    fn covers_every_item_exactly_once() {
        let plans = plan_batches(1_000, 64);
        let covered: usize = plans.iter().map(|p| p.len()).sum();
        assert_eq!(covered, 1_000);
        for pair in plans.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn degenerate_inputs_produce_no_batches() {
        assert!(plan_batches(0, 64).is_empty());
        assert!(plan_batches(10, 0).is_empty());
    }
}
