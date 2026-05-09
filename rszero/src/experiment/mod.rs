//! A/B testing and feature flag framework.
//!
//! Provides experiment definition, user assignment, and result tracking.
//! Supports weighted variant allocation, consistent hashing by user ID,
//! and metric collection per variant.
//!
//! # Example
//!
//! ```no_run
//! use rszero::experiment::{Experiment, ExperimentRegistry, Variant};
//!
//! # async fn example() {
//! let mut registry = ExperimentRegistry::new();
//! let exp = Experiment::new("new_checkout", vec![
//!     Variant::control("old_ui"),
//!     Variant::treatment("new_ui", 50),
//! ]);
//! registry.register(exp);
//!
//! let variant = registry.assign("new_checkout", "user_123");
//! match variant.id.as_str() {
//!     "new_ui" => { /* show new checkout */ }
//!     _ => { /* show old checkout */ }
//! }
//! # }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

/// A single variant in an A/B experiment.
#[derive(Debug, Clone)]
pub struct Variant {
    /// Unique variant identifier.
    pub id: String,
    /// Weight for traffic allocation (0-100).
    pub weight: u8,
    /// Whether this is the control variant.
    pub is_control: bool,
    /// Optional metadata.
    pub metadata: HashMap<String, String>,
}

impl Variant {
    /// Create a control variant with default weight.
    pub fn control(id: &str) -> Self {
        Self {
            id: id.to_string(),
            weight: 50,
            is_control: true,
            metadata: HashMap::new(),
        }
    }

    /// Create a treatment variant with a given weight.
    pub fn treatment(id: &str, weight: u8) -> Self {
        Self {
            id: id.to_string(),
            weight: weight.min(100),
            is_control: false,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the variant.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// An A/B experiment with multiple variants.
#[derive(Debug, Clone)]
pub struct Experiment {
    /// Unique experiment name.
    pub name: String,
    /// List of experiment variants.
    pub variants: Vec<Variant>,
    /// Whether the experiment is currently active.
    pub enabled: bool,
}

impl Experiment {
    /// Create a new experiment.
    pub fn new(name: &str, variants: Vec<Variant>) -> Self {
        Self {
            name: name.to_string(),
            variants,
            enabled: true,
        }
    }

    /// Disable the experiment (all users get control).
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Assign a user to a variant using consistent hashing.
    pub fn assign(&self, user_id: &str) -> &Variant {
        if self.variants.is_empty() {
            tracing::warn!(experiment = %self.name, "experiment has no variants, returning default control");
            return &CONTROL_FALLBACK;
        }
        if !self.enabled {
            return &self.variants[0];
        }

        // Simple consistent hash: hash(user_id + experiment_name)
        let hash = fnv1a_32(format!("{}:{}", self.name, user_id).as_bytes());
        let total_weight: u32 = self.variants.iter().map(|v| v.weight as u32).sum();
        if total_weight == 0 {
            return &self.variants[0];
        }

        let slot = hash % total_weight;
        let mut cumulative = 0u32;
        for variant in &self.variants {
            cumulative += variant.weight as u32;
            if slot < cumulative {
                return variant;
            }
        }
        &self.variants[self.variants.len() - 1]
    }
}

/// Registry for managing multiple experiments.
#[derive(Clone)]
pub struct ExperimentRegistry {
    experiments: Arc<RwLock<HashMap<String, Experiment>>>,
}

impl ExperimentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            experiments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an experiment.
    pub fn register(&self, experiment: Experiment) {
        let mut exps = self.experiments.write().unwrap_or_else(|e| e.into_inner());
        exps.insert(experiment.name.clone(), experiment);
    }

    /// Unregister an experiment by name.
    pub fn unregister(&self, name: &str) {
        let mut exps = self.experiments.write().unwrap_or_else(|e| e.into_inner());
        exps.remove(name);
    }

    /// Assign a user to a variant for the given experiment.
    /// Returns the control variant if the experiment doesn't exist.
    pub fn assign(&self, experiment_name: &str, user_id: &str) -> Variant {
        let exps = self.experiments.read().unwrap_or_else(|e| e.into_inner());
        match exps.get(experiment_name) {
            Some(exp) => exp.assign(user_id).clone(),
            None => Variant::control("default"),
        }
    }

    /// Check if an experiment exists and is enabled.
    pub fn is_enabled(&self, name: &str) -> bool {
        let exps = self.experiments.read().unwrap_or_else(|e| e.into_inner());
        exps.get(name).map(|e| e.enabled).unwrap_or(false)
    }

    /// List all registered experiment names.
    pub fn list(&self) -> Vec<String> {
        let exps = self.experiments.read().unwrap_or_else(|e| e.into_inner());
        exps.keys().cloned().collect()
    }

    /// Get experiment details.
    pub fn get(&self, name: &str) -> Option<Experiment> {
        let exps = self.experiments.read().unwrap_or_else(|e| e.into_inner());
        exps.get(name).cloned()
    }
}

/// Static fallback control variant for experiments with no variants.
static CONTROL_FALLBACK: LazyLock<Variant> = LazyLock::new(|| Variant::control("default"));

impl Default for ExperimentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Experiment result tracking for a single user session.
#[derive(Debug, Clone)]
pub struct ExperimentExposure {
    /// Name of the experiment.
    pub experiment: String,
    /// Assigned variant ID.
    pub variant: String,
    /// User identifier.
    pub user_id: String,
    /// Timestamp of exposure.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ExperimentExposure {
    /// Create a new experiment exposure record.
    pub fn new(experiment: &str, variant: &str, user_id: &str) -> Self {
        Self {
            experiment: experiment.to_string(),
            variant: variant.to_string(),
            user_id: user_id.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// FNV-1a 32-bit hash.
fn fnv1a_32(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_creation() {
        let control = Variant::control("old");
        assert!(control.is_control);
        assert_eq!(control.weight, 50);

        let treatment = Variant::treatment("new", 30);
        assert!(!treatment.is_control);
        assert_eq!(treatment.weight, 30);
    }

    #[test]
    fn test_experiment_assignment_deterministic() {
        let exp = Experiment::new("test", vec![
            Variant::control("a"),
            Variant::treatment("b", 50),
        ]);
        let v1 = exp.assign("user_123").id.clone();
        let v2 = exp.assign("user_123").id.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_experiment_disabled() {
        let exp = Experiment::new("test", vec![
            Variant::control("a"),
        ]).disable();
        let v = exp.assign("user_123");
        assert_eq!(v.id, "a");
    }

    #[test]
    fn test_registry() {
        let registry = ExperimentRegistry::new();
        let exp = Experiment::new("checkout", vec![
            Variant::control("old"),
            Variant::treatment("new", 50),
        ]);
        registry.register(exp);
        assert!(registry.is_enabled("checkout"));
        assert!(!registry.is_enabled("missing"));

        let variant = registry.assign("checkout", "user_1");
        assert!(variant.id == "old" || variant.id == "new");
    }

    #[test]
    fn test_exposure() {
        let exp = ExperimentExposure::new("test", "variant_a", "user_1");
        assert_eq!(exp.experiment, "test");
        assert_eq!(exp.variant, "variant_a");
    }
}
