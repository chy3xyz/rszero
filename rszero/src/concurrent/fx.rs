//! Functional stream processing — replicates go-zero's `fx` module.
//!
//! Provides a builder-pattern API for stream transformations:
//! `from(iter).map(f).filter(f).head(n).walk(f).done()`

/// Functional stream processor with type-safe transformations.
pub struct FxStream<T> {
    items: Vec<T>,
}

impl<T> FxStream<T> {
    /// Create a stream from a Vec.
    pub fn from(items: Vec<T>) -> Self {
        Self { items }
    }

    /// Transform each item via the mapper function.
    pub fn map<U, F>(self, f: F) -> FxStream<U>
    where
        F: Fn(T) -> U,
    {
        FxStream {
            items: self.items.into_iter().map(f).collect(),
        }
    }

    /// Filter items that don't satisfy the predicate.
    pub fn filter<F>(self, f: F) -> FxStream<T>
    where
        F: Fn(&T) -> bool,
    {
        FxStream {
            items: self.items.into_iter().filter(f).collect(),
        }
    }

    /// Take the first n items.
    pub fn head(self, n: usize) -> FxStream<T> {
        FxStream {
            items: self.items.into_iter().take(n).collect(),
        }
    }

    /// Execute a side-effect function for each item.
    pub fn walk<F>(self, f: F) -> FxStream<T>
    where
        F: Fn(&T),
    {
        for item in &self.items {
            f(item);
        }
        self
    }

    /// Collect all items into a Vec.
    pub fn done(self) -> Vec<T> {
        self.items
    }

    /// Reduce items to a single value.
    pub fn reduce<U, F>(self, f: F) -> Option<U>
    where
        T: Into<U>,
        F: Fn(U, T) -> U,
    {
        let mut iter = self.items.into_iter();
        let first = iter.next().map(|v| v.into())?;
        Some(iter.fold(first, f))
    }
}

/// Create an fx stream from a Vec.
pub fn from<T>(items: Vec<T>) -> FxStream<T> {
    FxStream::from(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_and_head() {
        let result = from(vec![1, 2, 3, 4, 5, 6])
            .filter(|x| x % 2 == 0)
            .head(2)
            .done();
        assert_eq!(result, vec![2, 4]);
    }

    #[test]
    fn test_map_transform() {
        let result = from(vec![1, 2, 3])
            .map(|x| x * 2)
            .done();
        assert_eq!(result, vec![2, 4, 6]);
    }

    #[test]
    fn test_walk_side_effect() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let sum = std::sync::Arc::new(AtomicUsize::new(0));
        let sum_clone = sum.clone();

        from(vec![1, 2, 3])
            .walk(move |x| { sum_clone.fetch_add(*x, Ordering::SeqCst); })
            .done();

        assert_eq!(sum.load(Ordering::SeqCst), 6);
    }

    #[test]
    fn test_from_done() {
        let items = vec![1, 2, 3];
        let result = from(items.clone()).done();
        assert_eq!(result, items);
    }

    #[test]
    fn test_reduce() {
        let sum = from(vec![1, 2, 3, 4, 5])
            .reduce(|acc, x| acc + x);
        assert_eq!(sum, Some(15));

        let empty: Option<i32> = from(Vec::<i32>::new()).reduce(|acc, x| acc + x);
        assert_eq!(empty, None);
    }
}
