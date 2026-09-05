//! The Prometheus endpoint.
//!
//! The `metrics` crate is a facade: any crate in the workspace can call
//! `counter!` or `histogram!` without knowing an exporter exists. Installing
//! one is a process-wide decision, so it happens once, here, and the binaries
//! call it at startup.
//!
//! Without a recorder installed every macro is a no-op, which is exactly what
//! tests want — they should not race over a global.

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

/// Buckets for request latency, in seconds.
///
/// Chosen around what we actually care about: the p95 target for everything
/// except the model call is 400ms, so the buckets are dense either side of it
/// and sparse out in the tail where a request is already too slow to argue
/// about.
const LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.4, 0.8, 1.5, 3.0, 6.0, 15.0, 30.0, 60.0,
];

/// Install the recorder for this process.
///
/// Returns the handle used to render the endpoint. Calling it twice fails,
/// which is right: a second recorder would silently take every measurement the
/// first one was reporting.
pub fn install() -> anyhow::Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Suffix("_duration_seconds".to_owned()),
            LATENCY_BUCKETS,
        )?
        .set_buckets_for_metric(
            Matcher::Suffix("_latency_seconds".to_owned()),
            LATENCY_BUCKETS,
        )?;

    let handle = builder.install_recorder()?;
    describe();
    Ok(handle)
}

/// Names and units, so a dashboard does not have to guess what a number means.
fn describe() {
    use metrics::{describe_counter, describe_gauge, describe_histogram, Unit};

    describe_counter!(
        "http_requests_total",
        "Requests served, by route, method and status"
    );
    describe_histogram!(
        "http_request_duration_seconds",
        Unit::Seconds,
        "How long a request took, end to end"
    );
    describe_counter!(
        "provider_requests_total",
        "Calls to a model provider, by provider, model and outcome"
    );
    describe_histogram!(
        "provider_latency_seconds",
        Unit::Seconds,
        "How long a model provider took to answer"
    );
    describe_histogram!(
        "retrieval_duration_seconds",
        Unit::Seconds,
        "How long a search took, from question to ranked passages"
    );
    describe_gauge!("jobs_pending", "Jobs waiting to be picked up");
    describe_gauge!("jobs_running", "Jobs a worker currently holds");
    describe_gauge!("jobs_dead", "Jobs that exhausted their attempts");
    describe_counter!(
        "jobs_failed_total",
        "Job attempts that failed, by kind and whether they will be retried"
    );
    describe_counter!(
        "usage_tokens_total",
        "Tokens billed, by direction. No tenant label: this endpoint is scraped \
         into a store with a different retention policy from our own database."
    );
}
