//! Logging, request correlation, and intentionally low-volume trace export.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderValue, Response};
use axum::middleware::Next;
use axum::response::Response as AxumResponse;
use futures_executor::block_on;
use opentelemetry::trace::{SpanId, Status, TraceContextExt, TraceId, TracerProvider};
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter, SpanProcessor};
use tower_http::request_id::RequestId;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const EXPORT_FAILURE_LOG_INTERVAL_SECS: u64 = 60;
const ROOT_SPAN_PREFIX: &str = "HTTP ";

static TRACE_CONFIG: OnceLock<Option<TraceConfig>> = OnceLock::new();

#[derive(Clone, Debug)]
struct TraceConfig {
    slow_threshold: Duration,
    sample_rate: f64,
    force_secret: Option<String>,
    buffer_max_traces: usize,
    buffer_max_spans: usize,
    export_queue: usize,
    export_timeout: Duration,
}

impl TraceConfig {
    fn from_env() -> Option<(Self, String)> {
        let endpoint = std::env::var("TRACE_OTLP_ENDPOINT")
            .ok()
            .filter(|endpoint| !endpoint.trim().is_empty())?;
        Some((
            Self {
                slow_threshold: Duration::from_millis(env_u64("TRACE_SLOW_THRESHOLD_MS", 500)),
                sample_rate: env_f64("TRACE_SAMPLE_RATE", 0.0).clamp(0.0, 1.0),
                force_secret: std::env::var("TRACE_FORCE_SECRET")
                    .ok()
                    .filter(|secret| !secret.is_empty()),
                buffer_max_traces: env_usize("TRACE_BUFFER_MAX_TRACES", 1024),
                buffer_max_spans: env_usize("TRACE_BUFFER_MAX_SPANS", 32),
                export_queue: env_usize("TRACE_EXPORT_QUEUE", 256),
                export_timeout: Duration::from_millis(env_u64("TRACE_EXPORT_TIMEOUT_MS", 1_000)),
            },
            endpoint,
        ))
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn host_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "unknown".into())
}

/// Initializes logs and, when `TRACE_OTLP_ENDPOINT` is configured, OTLP/HTTP tracing.
///
/// The disabled default deliberately leaves the subscriber exactly as lightweight as the
/// web_template baseline. The enabled path records each request locally, then makes a tail
/// decision only when its root span closes.
pub fn init(service_name: &'static str) {
    let configured = TraceConfig::from_env();
    let trace_config = configured.as_ref().map(|(config, _)| config.clone());
    let _ = TRACE_CONFIG.set(trace_config);

    let fmt = tracing_subscriber::fmt::layer().with_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "ronitnath=debug,tower_http=info".into()),
    );

    if let Some((config, endpoint)) = configured {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .with_timeout(config.export_timeout)
            .build()
            .expect("TRACE_OTLP_ENDPOINT must be a valid OTLP/HTTP endpoint");
        let processor = DeferredTailProcessor::new(exporter, config.clone());
        let provider = SdkTracerProvider::builder()
            .with_resource(
                Resource::builder_empty()
                    .with_attributes([
                        KeyValue::new("service.name", service_name),
                        KeyValue::new("service.version", env!("GIT_HASH")),
                        KeyValue::new("host.name", host_name()),
                    ])
                    .build(),
            )
            .with_span_processor(processor)
            .build();
        let tracer = provider.tracer(service_name);
        global::set_tracer_provider(provider);
        tracing_subscriber::registry()
            .with(fmt)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();
        tracing::info!(service_name, "OTLP trace export enabled");
    } else {
        tracing_subscriber::registry().with(fmt).init();
    }
}

/// Builds the request root span used by [`tower_http::trace::TraceLayer`].
pub fn make_span(request: &Request) -> Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("-")
        .to_string();
    let route = matched_route(request);
    let method = request.method().to_string();
    let forced = force_trace(request);
    let otel_name = format!("HTTP {method} {route}");

    let span = tracing::info_span!(
        "HTTP request",
        otel.name = %otel_name,
        http.request.method = %method,
        http.route = %route,
        request_id,
        trace.force = forced,
        trace_id = tracing::field::Empty,
        span_id = tracing::field::Empty,
        status = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );
    record_trace_context(&span);
    span
}

/// Adds response data to the root span and returns the trace id for a valid forced request.
/// This middleware is deliberately inside `TraceLayer`, so its own logs share the request span.
pub async fn record_response(request: Request, next: Next) -> AxumResponse {
    let forced = force_trace(&request);
    let response = next.run(request).await;
    let span = Span::current();
    let status = response.status();
    span.record("status", status.as_u16());
    span.set_attribute("http.response.status_code", status.as_u16() as i64);
    if status.is_server_error() {
        span.set_status(Status::error(status.to_string()));
    } else {
        span.set_status(Status::Ok);
    }

    if forced {
        let trace_id = trace_id(&span);
        let mut response = response;
        if let Ok(value) = HeaderValue::from_str(&trace_id) {
            response.headers_mut().insert("x-trace-id", value);
        }
        response
    } else {
        response
    }
}

/// Records the outcome of a request against its [`make_span`] span.
pub fn on_response<B>(response: &Response<B>, latency: Duration, span: &Span) {
    span.record("status", response.status().as_u16());
    span.record("latency_ms", latency.as_millis() as u64);
    tracing::info!(
        status = response.status().as_u16(),
        latency_ms = latency.as_millis() as u64,
        "response"
    );
}

fn matched_route(request: &Request) -> String {
    request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "<unmatched>".into())
}

fn force_trace(request: &Request) -> bool {
    let Some(config) = TRACE_CONFIG.get().and_then(Option::as_ref) else {
        return false;
    };
    let Some(secret) = &config.force_secret else {
        return false;
    };
    request
        .headers()
        .get("x-force-trace")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|provided| constant_time_eq(provided.as_bytes(), secret.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut difference = u64::try_from(left.len() ^ right.len()).unwrap_or(u64::MAX);
    for index in 0..max_len {
        difference |= u64::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

fn record_trace_context(span: &Span) {
    let context = span.context();
    let trace_span = context.span();
    let context = trace_span.span_context();
    if context.is_valid() {
        span.record("trace_id", tracing::field::display(context.trace_id()));
        span.record("span_id", tracing::field::display(context.span_id()));
    }
}

fn trace_id(span: &Span) -> String {
    let context = span.context();
    let trace_span = context.span();
    let context = trace_span.span_context();
    if context.is_valid() {
        context.trace_id().to_string()
    } else {
        String::new()
    }
}

/// Redacts capability credentials while preserving route shape for diagnostics.
/// This only changes telemetry; axum continues routing on the original URI.
pub fn sanitize_path(path: &str) -> String {
    for (prefix, placeholder) in [("/e/", "{token}"), ("/api/e/", "{token}")] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let suffix = rest.find('/').map_or("", |at| &rest[at..]);
            return format!("{prefix}{placeholder}{suffix}");
        }
    }
    if let Some(feed) = path.strip_prefix("/calendar/")
        && !feed.contains('/')
        && feed.ends_with(".ics")
    {
        return "/calendar/{feed}.ics".into();
    }
    if let Some(event_ref) = path.strip_prefix("/events/")
        && let Some(token) = event_ref.strip_suffix("/ics")
        && !token.contains('/')
    {
        return "/events/{token}/ics".into();
    }
    path.to_owned()
}

#[derive(Debug)]
enum ExportMessage {
    Trace(Vec<SpanData>),
    Shutdown,
}

/// A bounded, in-process tail sampler. `SpanProcessor::on_end` only performs map/channel
/// bookkeeping; all OTLP I/O happens on its dedicated worker thread.
struct DeferredTailProcessor<T: SpanExporter + 'static> {
    config: TraceConfig,
    buffers: Mutex<HashMap<TraceId, Vec<SpanData>>>,
    sender: mpsc::SyncSender<ExportMessage>,
    exporter: Arc<Mutex<T>>,
}

impl<T: SpanExporter + 'static> fmt::Debug for DeferredTailProcessor<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeferredTailProcessor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<T: SpanExporter + 'static> DeferredTailProcessor<T> {
    fn new(exporter: T, config: TraceConfig) -> Self {
        let (sender, receiver) = mpsc::sync_channel(config.export_queue);
        let exporter = Arc::new(Mutex::new(exporter));
        let worker_exporter = Arc::clone(&exporter);
        thread::Builder::new()
            .name("otlp-trace-export".into())
            .spawn(move || export_worker(receiver, worker_exporter))
            .expect("failed to start OTLP export worker");
        Self {
            config,
            buffers: Mutex::new(HashMap::new()),
            sender,
            exporter,
        }
    }
}

impl<T: SpanExporter + 'static> SpanProcessor for DeferredTailProcessor<T> {
    fn on_start(&self, _span: &mut opentelemetry_sdk::trace::Span, _cx: &opentelemetry::Context) {}

    fn on_end(&self, mut span: SpanData) {
        if !span.span_context.is_sampled() {
            return;
        }
        let trace_id = span.span_context.trace_id();
        let root =
            span.parent_span_id == SpanId::INVALID && span.name.starts_with(ROOT_SPAN_PREFIX);
        let mut buffers = match self.buffers.lock() {
            Ok(buffers) => buffers,
            Err(_) => return,
        };
        if !buffers.contains_key(&trace_id) && buffers.len() >= self.config.buffer_max_traces {
            return;
        }
        let buffer = buffers.entry(trace_id).or_default();
        if buffer.len() >= self.config.buffer_max_spans {
            if root {
                buffers.remove(&trace_id);
            }
            return;
        }
        if root {
            let decision = sampling_decision(&span, &self.config);
            span.attributes
                .push(KeyValue::new("trace.sample.reason", decision.reason));
            buffer.push(span);
            if decision.export {
                if let Some(trace) = buffers.remove(&trace_id) {
                    let _ = self.sender.try_send(ExportMessage::Trace(trace));
                }
            } else {
                buffers.remove(&trace_id);
            }
        } else {
            buffer.push(span);
        }
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> opentelemetry_sdk::error::OTelSdkResult {
        let _ = self.sender.try_send(ExportMessage::Shutdown);
        Ok(())
    }

    fn set_resource(&mut self, resource: &Resource) {
        if let Ok(mut exporter) = self.exporter.lock() {
            exporter.set_resource(resource);
        }
    }
}

struct SamplingDecision {
    export: bool,
    reason: &'static str,
}

fn sampling_decision(root: &SpanData, config: &TraceConfig) -> SamplingDecision {
    let status = attribute_u16(&root.attributes, "http.response.status_code").unwrap_or(0);
    let duration = root
        .end_time
        .duration_since(root.start_time)
        .unwrap_or_default();
    sampling_decision_for(
        status,
        duration,
        attribute_bool(&root.attributes, "trace.force"),
        root.span_context.trace_id(),
        config,
    )
}

fn sampling_decision_for(
    status: u16,
    duration: Duration,
    forced: bool,
    trace_id: TraceId,
    config: &TraceConfig,
) -> SamplingDecision {
    if status >= 500 {
        return SamplingDecision {
            export: true,
            reason: "server_error",
        };
    }
    if duration > config.slow_threshold {
        return SamplingDecision {
            export: true,
            reason: "slow",
        };
    }
    if forced {
        return SamplingDecision {
            export: true,
            reason: "forced",
        };
    }
    if sampled(trace_id, config.sample_rate) {
        SamplingDecision {
            export: true,
            reason: "rate",
        }
    } else {
        SamplingDecision {
            export: false,
            reason: "drop",
        }
    }
}

fn attribute_u16(attributes: &[KeyValue], key: &str) -> Option<u16> {
    attributes.iter().find_map(|attribute| {
        (attribute.key.as_str() == key)
            .then(|| match &attribute.value {
                opentelemetry::Value::I64(value) => u16::try_from(*value).ok(),
                _ => None,
            })
            .flatten()
    })
}

fn attribute_bool(attributes: &[KeyValue], key: &str) -> bool {
    attributes.iter().any(|attribute| {
        attribute.key.as_str() == key && matches!(attribute.value, opentelemetry::Value::Bool(true))
    })
}

fn sampled(trace_id: TraceId, rate: f64) -> bool {
    if rate <= 0.0 {
        return false;
    }
    if rate >= 1.0 {
        return true;
    }
    let bytes = trace_id.to_bytes();
    let prefix = u64::from_be_bytes(
        bytes[..8]
            .try_into()
            .expect("trace id prefix is eight bytes"),
    );
    (prefix as f64 / u64::MAX as f64) < rate
}

fn export_worker<T: SpanExporter + 'static>(
    receiver: mpsc::Receiver<ExportMessage>,
    exporter: Arc<Mutex<T>>,
) {
    let last_failure = AtomicU64::new(0);
    while let Ok(message) = receiver.recv() {
        match message {
            ExportMessage::Trace(trace) => {
                let result = exporter
                    .lock()
                    .map_err(|_| "exporter mutex poisoned".to_string())
                    .and_then(|exporter| {
                        block_on(exporter.export(trace)).map_err(|error| error.to_string())
                    });
                if let Err(error) = result {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let previous = last_failure.load(Ordering::Relaxed);
                    if now.saturating_sub(previous) >= EXPORT_FAILURE_LOG_INTERVAL_SECS
                        && last_failure
                            .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
                            .is_ok()
                    {
                        tracing::warn!(%error, "OTLP trace export failed; dropping queued trace");
                    }
                }
            }
            ExportMessage::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TraceConfig, constant_time_eq, sampled, sampling_decision_for, sanitize_path};
    use opentelemetry::trace::TraceId;
    use std::time::Duration;

    #[test]
    fn capability_paths_are_redacted_without_losing_route_shape() {
        let sentinel = "SENTINEL-CAPABILITY-CREDENTIAL";
        for (raw, expected) in [
            (format!("/e/{sentinel}"), "/e/{token}"),
            (format!("/e/{sentinel}/claim"), "/e/{token}/claim"),
            (
                format!("/e/{sentinel}/photos/42/medium"),
                "/e/{token}/photos/42/medium",
            ),
            (format!("/api/e/{sentinel}/rsvp"), "/api/e/{token}/rsvp"),
            (format!("/calendar/{sentinel}.ics"), "/calendar/{feed}.ics"),
            (format!("/events/{sentinel}/ics"), "/events/{token}/ics"),
        ] {
            let sanitized = sanitize_path(&raw);
            assert_eq!(sanitized, expected);
            assert!(!sanitized.contains(sentinel));
        }
        assert_eq!(sanitize_path("/calendar"), "/calendar");
        assert_eq!(sanitize_path("/events/42"), "/events/42");
    }

    #[test]
    fn rate_sampling_is_deterministic_and_honors_its_bounds() {
        let trace_id = TraceId::from_bytes([0; 16]);
        assert!(!sampled(trace_id, 0.0));
        assert!(sampled(trace_id, 1.0));
        assert!(sampled(trace_id, 0.5));
        assert!(!sampled(TraceId::from_bytes([0xff; 16]), 0.5));
    }

    #[test]
    fn secret_comparison_requires_every_byte_to_match() {
        assert!(constant_time_eq(b"correct", b"correct"));
        assert!(!constant_time_eq(b"correct", b"wrong"));
        assert!(!constant_time_eq(b"correct", b"correct-but-longer"));
    }

    #[test]
    fn defaults_are_targeted_only() {
        let config = TraceConfig {
            slow_threshold: Duration::from_millis(500),
            sample_rate: 0.0,
            force_secret: None,
            buffer_max_traces: 1024,
            buffer_max_spans: 32,
            export_queue: 256,
            export_timeout: Duration::from_secs(1),
        };
        assert_eq!(config.sample_rate, 0.0);
    }

    #[test]
    fn tail_sampling_keeps_errors_slow_requests_and_forced_requests_at_rate_zero() {
        let config = TraceConfig {
            slow_threshold: Duration::from_millis(500),
            sample_rate: 0.0,
            force_secret: None,
            buffer_max_traces: 1,
            buffer_max_spans: 1,
            export_queue: 1,
            export_timeout: Duration::from_secs(1),
        };
        let trace_id = TraceId::from_bytes([0xff; 16]);
        assert_eq!(
            sampling_decision_for(500, Duration::ZERO, false, trace_id, &config).reason,
            "server_error"
        );
        assert_eq!(
            sampling_decision_for(200, Duration::from_millis(501), false, trace_id, &config).reason,
            "slow"
        );
        assert_eq!(
            sampling_decision_for(200, Duration::ZERO, true, trace_id, &config).reason,
            "forced"
        );
        assert!(!sampling_decision_for(200, Duration::ZERO, false, trace_id, &config).export);
    }
}
