use crate::Error;
use opentelemetry::trace::TraceId;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, exemplar::HistogramWithExemplars, family::Family},
    registry::{Registry, Unit},
};
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub reconcile: ReconcileMetrics,
    pub registry: Arc<Registry>,
}

impl Default for Metrics {
    fn default() -> Self {
        let mut registry = Registry::with_prefix("controller");
        let reconcile = ReconcileMetrics::default().register(&mut registry);
        Self {
            registry: Arc::new(registry),
            reconcile,
        }
    }
}

#[derive(Clone)]
pub struct ReconcileMetrics {
    pub runs: Family<ReconcileLabels, Counter>,
    pub failures: Family<ErrorLabels, Counter>,
    pub duration: HistogramWithExemplars<ReconcileLabels>,
}

impl Default for ReconcileMetrics {
    fn default() -> Self {
        Self {
            runs: Family::<ReconcileLabels, Counter>::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            duration: HistogramWithExemplars::new(
                [0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter(),
            ),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ReconcileLabels {
    pub api_version: String,
    pub api_kind: String,
    pub trace_id: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ErrorLabels {
    pub api_version: String,
    pub api_kind: String,
    pub instance: String,
    pub error: String,
}

impl ReconcileMetrics {
    /// Register API metrics to start tracking them.
    pub fn register(self, r: &mut Registry) -> Self {
        r.register_with_unit(
            "duration",
            "reconcile duration",
            Unit::Seconds,
            self.duration.clone(),
        );
        r.register("failures", "reconciliation errors", self.failures.clone());
        r.register("runs", "reconciliations", self.runs.clone());
        self
    }

    pub fn set_failure(&self, api_version: String, api_kind: String, name: String, e: &Error) {
        self.failures
            .get_or_create(&ErrorLabels {
                api_version,
                api_kind,
                instance: name,
                error: e.metric_label(),
            })
            .inc();
    }

    pub fn count_and_measure(
        &self,
        api_version: String,
        api_kind: String,
        trace_id: &TraceId,
    ) -> ReconcileMeasurer {
        let labels = &ReconcileLabels {
            api_version,
            api_kind,
            trace_id: trace_id.to_string(),
        };

        self.runs.get_or_create(labels).inc();

        ReconcileMeasurer {
            start: Instant::now(),
            labels: labels.clone(),
            metric: self.duration.clone(),
        }
    }
}

/// Smart function duration measurer
///
/// Relies on Drop to calculate duration and register the observation in the histogram
pub struct ReconcileMeasurer {
    start: Instant,
    labels: ReconcileLabels,
    metric: HistogramWithExemplars<ReconcileLabels>,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        self.metric.observe(
            duration,
            Some(self.labels.clone()),
            Some(std::time::SystemTime::now()),
        );
    }
}
