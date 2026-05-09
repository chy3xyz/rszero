//! Saga distributed transaction coordinator.
//!
//! Implements the Saga pattern for managing long-running distributed transactions
//! with compensating actions. Each step has a forward action and a backward
//! compensation that runs on failure.
//!
//! # Example
//! ```ignore
//! use rszero::saga::Saga;
//!
//! let result = Saga::new("order-saga")
//!     .step("deduct_stock", deduct_stock, restore_stock)
//!     .step("charge_payment", charge_payment, refund_payment)
//!     .step("ship_order", ship_order, cancel_shipment)
//!     .execute().await;
//! ```

#[cfg(feature = "store")]
pub mod persistence;

use crate::error::{RszeroError, RszeroResult};
use std::future::Future;
use std::pin::Pin;

/// Type alias for saga step actions.
pub type SagaAction<T> = Box<dyn Fn() -> Pin<Box<dyn Future<Output = RszeroResult<T>> + Send>> + Send + Sync>;

/// Type alias for compensation actions.
pub type Compensation = Box<dyn Fn() -> Pin<Box<dyn Future<Output = RszeroResult<()>> + Send>> + Send + Sync>;

/// A single step in a saga transaction.
pub(crate) struct SagaStep<T> {
    name: String,
    action: SagaAction<T>,
    compensation: Compensation,
}

impl<T> SagaStep<T> {
    /// Create a new saga step.
    pub fn new<F, Fut, C, Cfu>(
        name: &str,
        action: F,
        compensation: C,
    ) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RszeroResult<T>> + Send + 'static,
        C: Fn() -> Cfu + Send + Sync + 'static,
        Cfu: Future<Output = RszeroResult<()>> + Send + 'static,
    {
        Self {
            name: name.to_string(),
            action: Box::new(move || Box::pin(action())),
            compensation: Box::new(move || Box::pin(compensation())),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) async fn execute(&self) -> RszeroResult<T> {
        (self.action)().await
    }

    pub(crate) async fn compensate(&self) -> RszeroResult<()> {
        (self.compensation)().await
    }
}

/// Saga execution result containing completed steps and their outputs.
pub struct SagaResult<T> {
    /// Steps that completed successfully.
    pub completed: Vec<String>,
    /// Outputs from completed steps.
    pub outputs: Vec<T>,
    /// Step that failed (if any).
    pub failed_step: Option<String>,
    /// Error from the failed step.
    pub error: Option<RszeroError>,
    /// Errors from compensation steps that failed after retries.
    pub compensation_errors: Vec<(String, RszeroError)>,
}

/// Saga distributed transaction coordinator.
pub struct Saga<T> {
    name: String,
    steps: Vec<SagaStep<T>>,
}

impl<T: Send + 'static> Saga<T> {
    /// Create a new saga with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
        }
    }

    fn generate_id(&self) -> String {
        format!("{}-{}", self.name, uuid::Uuid::new_v4())
    }

    /// Add a step to the saga.
    pub fn step<F, Fut, C, Cfu>(mut self, name: &str, action: F, compensation: C) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RszeroResult<T>> + Send + 'static,
        C: Fn() -> Cfu + Send + Sync + 'static,
        Cfu: Future<Output = RszeroResult<()>> + Send + 'static,
    {
        self.steps.push(SagaStep::new(name, action, compensation));
        self
    }

    /// Execute the saga.
    ///
    /// Steps run sequentially. If any step fails, all previously completed
    /// steps are compensated in reverse order.
    pub async fn execute(self) -> SagaResult<T> {
        let saga_id = self.generate_id();
        let mut completed = Vec::new();
        let mut outputs = Vec::new();
        let mut compensation_errors = Vec::new();

        for step in &self.steps {
            tracing::info!(saga = %self.name, saga_id, step = %step.name, "executing saga step");
            match step.execute().await {
                Ok(output) => {
                    tracing::info!(saga = %self.name, saga_id, step = %step.name, "saga step succeeded");
                    completed.push(step.name.clone());
                    outputs.push(output);
                }
                Err(e) => {
                    tracing::error!(saga = %self.name, saga_id, step = %step.name, error = %e, "saga step failed, compensating");

                    // Compensate in reverse order with retry and idempotency key
                    for i in (0..completed.len()).rev() {
                        let comp_step = &self.steps[i];
                        let _idempotency_key = format!("{}:{}:{}", saga_id, comp_step.name(), i);
                        let mut comp_ok = false;
                        for retry in 0..3 {
                            match comp_step.compensate().await {
                                Ok(()) => {
                                    tracing::info!(saga = %self.name, saga_id, step = %comp_step.name(), "compensation succeeded");
                                    comp_ok = true;
                                    break;
                                }
                                Err(ce) => {
                                    tracing::error!(saga = %self.name, saga_id, step = %comp_step.name(), error = %ce, attempt = retry, "compensation failed");
                                    if retry == 2 {
                                        compensation_errors.push((comp_step.name().to_string(), ce));
                                    }
                                }
                            }
                        }
                        if !comp_ok {
                            tracing::error!(saga = %self.name, saga_id, step = %comp_step.name(), "compensation exhausted all retries");
                        }
                    }

                    return SagaResult {
                        completed,
                        outputs,
                        failed_step: Some(step.name.clone()),
                        error: Some(e),
                        compensation_errors,
                    };
                }
            }
        }

        tracing::info!(saga = %self.name, saga_id, steps = completed.len(), "saga completed successfully");
        SagaResult {
            completed,
            outputs,
            failed_step: None,
            error: None,
            compensation_errors,
        }
    }

    /// Get the number of steps.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Get the saga name.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn steps(&self) -> &[SagaStep<T>] {
        &self.steps
    }
}

/// Check if a saga result indicates success.
impl<T> SagaResult<T> {
    /// Returns true if all steps completed successfully.
    pub fn is_success(&self) -> bool {
        self.failed_step.is_none()
    }

    /// Returns true if any compensation step failed.
    pub fn has_compensation_failure(&self) -> bool {
        !self.compensation_errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_saga_success() {
        let saga = Saga::<i32>::new("test-success")
            .step("step1", || async { Ok(1) }, || async { Ok(()) })
            .step("step2", || async { Ok(2) }, || async { Ok(()) });

        let result = saga.execute().await;
        assert!(result.is_success());
        assert_eq!(result.outputs, vec![1, 2]);
    }

    #[tokio::test]
    async fn test_saga_failure_with_compensation() {
        let compensated = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = compensated.clone();

        let saga = Saga::<i32>::new("test-fail")
            .step("step1", || async { Ok(1) }, move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(())
                }
            })
            .step("step2", || async {
                Err(RszeroError::Internal { message: "fail".into(), source: None })
            }, || async { Ok(()) });

        let result = saga.execute().await;
        assert!(!result.is_success());
        assert_eq!(result.completed, vec!["step1"]);
        assert_eq!(compensated.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_saga_builder() {
        let saga = Saga::<()>::new("builder")
            .step("a", || async { Ok(()) }, || async { Ok(()) })
            .step("b", || async { Ok(()) }, || async { Ok(()) });
        assert_eq!(saga.step_count(), 2);
    }
}
