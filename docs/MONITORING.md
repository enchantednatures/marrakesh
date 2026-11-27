# Monitoring Guide

Complete observability guide for marrakesh, including distributed tracing, metrics, and logs.

## Table of Contents

- [Observability Stack](#observability-stack)
- [Distributed Tracing](#distributed-tracing)
- [Metrics](#metrics)
- [Logging](#logging)
- [Alerting](#alerting)
- [Dashboards](#dashboards)
- [Production Monitoring](#production-monitoring)

---

## Observability Stack

```
Application
    ├─ Traces (OTLP/gRPC) ──────────────► OTel Collector ──► Jaeger
    ├─ Metrics (Prometheus /metrics) ───► Prometheus
    └─ Logs (structured JSON) ──────────► stdout / cluster log collector
```

**Local Development** (via make local-up):
- Jaeger: http://localhost:16686
- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (`admin` / `admin`)
- OpenTelemetry Collector: http://localhost:4317

**Production**:
- Deployed to Kubernetes
- Managed observability platform (Grafana Cloud, Datadog, etc.)

---

## Distributed Tracing

### Overview

Distributed tracing tracks requests as they flow through services, providing:

- **Request Lifecycle**: From ingress to response
- **Service Dependencies**: Which services are called
- **Performance Bottlenecks**: Where time is spent
- **Root Cause Analysis**: Why requests fail

### Accessing Jaeger

**Local**: http://localhost:16686

**Production**: URL of your Jaeger instance

### Jaeger UI Features

1. **Search Traces**: Filter by service, operation, tags
2. **Trace View**: Timeline of spans across services
3. **Span Details**: Logs, tags, timing
4. **Comparison**: Compare multiple traces

### Trace Propagation

**Knative Compatibility**:
- **B3 Propagation**: Standard for service meshes
- **Trace ID**: Correlates all spans in a request
- **Span ID**: Identifies individual operations

**Flow**:
```
Request enters → Trace ID generated
    ↓
HTTP Handler → Parent span created
    ↓
Redis Call → Child span created

    ↓
S3 Call → Child span created

    ↓
Response → Parent span closed
```

### Viewing Traces

#### By Trace ID

```bash
# Get trace ID from response headers
curl -I http://localhost:8080/health/live

# View trace in Jaeger
# Navigate to: http://localhost:16686/trace/<trace-id>
```

#### By Service/Operation

1. Open Jaeger UI: http://localhost:16686
2. Select Service: `marrakesh`
3. Select Operation: `/api/upload`, `/api/download`, etc.
4. Click "Find Traces"
5. Click on a trace to view details

### Custom Spans

Add custom spans to track business logic:

```rust
use tracing::{instrument, info_span, Level};

// Attribute-based
#[instrument(skip(state), fields(user_id = %user.id))]
pub async fn user_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    // ... handler logic
}

// Manual span
pub async fn complex_operation() -> Result<String, AppError> {
    let span = info_span!("complex_operation", input = "data");
    let _enter = span.enter();
    
    // Your operation here
    
    Ok("result".to_string())
}
```

### Trace Sampling

Reduce trace volume in production:

```bash
# Sample 10% of traces
export APP__TELEMETRY__SAMPLER=ratio:0.1

# Sample 1% of traces
export APP__TELEMETRY__SAMPLER=ratio:0.01

# Trace all requests (development)
export APP__TELEMETRY__SAMPLER=always
```

### Trace Analysis

**Common Patterns**:

- **High Latency**: Look for slow spans (red in Jaeger)
- **Errors**: Spans with error tags
- **Cascade Failures**: Failed spans causing downstream failures
- **N+1 Queries**: Many Redis calls in one request

**PromQL for Trace Metrics**:
```promql
# High error rate
rate(spans_dropped_total[5m]) > 0.1

# Slow requests
histogram_quantile(0.99, sum(rate(spans_duration_ms_bucket[5m])) by (le))
```

---

## Metrics

### Overview

Metrics track numerical values over time for:

- **Performance**: Request latency, throughput
- **Resources**: CPU, memory, disk
- **Business**: Requests served, errors, objects uploaded

### Built-in Metrics

The app records metrics with the `metrics` crate and exposes them through
`metrics-exporter-prometheus` at `GET /metrics`. Every metric carries the global
`service` label, whose value is the Rust crate name.

**HTTP metrics**:
```
http_requests_total (counter)              labels: service, status
http_request_duration_seconds (histogram)  labels: service, status
app_info (counter)                         labels: service
```

`status` is the HTTP status code. `method` and `path` are intentionally not
recorded: a path label can grow without bound on unmatched routes. Add them in
`src/routes.rs` if your traffic has a small, fixed route set.


**Kafka publisher metrics**:
```
kafka_events_published_total (counter)   labels: service, topic
kafka_events_failed_total (counter)      labels: service, topic, error_type
kafka_publish_latency_ms (histogram)     labels: service, topic
```





CPU, memory, and disk metrics are not exposed by the app; they come from
cluster monitoring (cAdvisor, kube-state-metrics, or your platform's container
metrics).

### Accessing Prometheus

**Local**: http://localhost:9090

**Production**: URL of your Prometheus instance

### PromQL Queries

> `$service` is the Grafana template variable used by the generated dashboard.
> When querying Prometheus directly, replace it with the crate name (for
> example `service="my_service"`).

**Request rate**:
```promql
# Requests per second
sum(rate(http_requests_total{service=~"$service"}[5m]))

# By status
sum by (status) (rate(http_requests_total{service=~"$service"}[5m]))

# p95 latency
histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket{service=~"$service"}[5m])))
```

**Error rate**:
```promql
# Requests per second failing with 5xx
sum(rate(http_requests_total{service=~"$service",status=~"5.."}[5m]))

# Error ratio (0..1)
sum(rate(http_requests_total{service=~"$service",status=~"5.."}[5m]))
  / clamp_min(sum(rate(http_requests_total{service=~"$service"}[5m])), 0.001)
```


**Kafka publisher**:
```promql
sum by (topic) (rate(kafka_events_published_total[5m]))
sum by (topic, error_type) (rate(kafka_events_failed_total[5m]))
histogram_quantile(0.99, sum by (le, topic) (rate(kafka_publish_latency_ms_bucket[5m])))
```




**Resource metrics** (from cluster monitoring, not the app):
```promql
# CPU usage by pod
sum by (pod) (rate(container_cpu_usage_seconds_total{namespace="marrakesh"}[5m]))

# Memory working set by pod
sum by (pod) (container_memory_working_set_bytes{namespace="marrakesh"})
```
If your stack uses kube-state-metrics, `kube_pod_container_resource_requests` and
`kube_pod_container_status_restarts_total` are also useful.

### Custom Metrics

Record application metrics with the `metrics` crate macros (the app already
installs the Prometheus recorder):

```rust
use metrics::{counter, gauge, histogram};

counter!("s3_upload_total").increment(1);
histogram!("s3_upload_size_bytes").record(data.len() as f64);
gauge!("s3_active_downloads").increment(1.0);
```

Pre-register descriptions in `crates/service-core/src/observability.rs` with
`describe_counter!`/`describe_histogram!` if the metric should appear before its
first sample. Histogram buckets are configured globally in `init_metrics`
(seconds by default); use `set_buckets_for_metric` for a different unit, as the
Kafka latency histogram does for milliseconds.

### Exporting Metrics

**Endpoint**: `GET /metrics`

**Format**: OpenMetrics / Prometheus text format

**Example**:
```bash
curl http://localhost:8080/metrics
```

Output:
```
# HELP http_requests_total Total number of HTTP requests
# TYPE http_requests_total counter
http_requests_total{service="my_service",status="200"} 45

# HELP http_request_duration_seconds HTTP request latency in seconds
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{service="my_service",status="200",le="0.005"} 42
http_request_duration_seconds_bucket{service="my_service",status="200",le="0.01"} 45
http_request_duration_seconds_bucket{service="my_service",status="200",le="0.025"} 45
...
http_request_duration_seconds_sum{service="my_service",status="200"} 0.15
http_request_duration_seconds_count{service="my_service",status="200"} 45
```

### Prometheus Scraping

**Prometheus Configuration** (`deploy/dev/observability/prometheus.yaml`):
```yaml
scrape_configs:
  - job_name: 'rust-service'
    static_configs:
      # Knative exposes 8080 directly; a Deployment Service fronts the pod
      # with workload_service_port.
      - targets: ['marrakesh.marrakesh.svc.cluster.local:8080']
    metrics_path: '/metrics'
    scrape_interval: 10s
```

**Kubernetes ServiceMonitor**:
```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: marrakesh
  namespace: marrakesh
spec:
  selector:
    matchLabels:

      serving.knative.dev/service: marrakesh

  endpoints:
  - port: http
    path: /metrics
    interval: 15s
```

---

## Logging

### Overview

Structured logs with context:

- **JSON Format**: Machine-readable
- **Trace Correlation**: Links logs to traces
- **Severity Levels**: TRACE, DEBUG, INFO, WARN, ERROR
- **Context Fields**: Request ID, user ID, etc.

### Log Format

**JSON Log Example**:
```json
{
  "timestamp": "2024-01-15T10:30:45.123Z",
  "level": "INFO",
  "target": "marrakesh::handlers::api",
  "message": "Request received",
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "4d33b4a7f3ba4b6c",
  "http.method": "GET",
  "http.path": "/api/upload",
  "http.status": 200,
  "http.duration_ms": 45
}
```

### Viewing Logs

**Local**:
```bash
# Follow logs
cargo run

# With debug output
RUST_LOG=debug cargo run

# Filter by module
RUST_LOG=marrakesh=debug,redis=info cargo run

# JSON logs with jq
RUST_LOG=info cargo run 2>&1 | jq '.'
```

**Kubernetes**:
```bash
# Pod logs
kubectl logs -f deployment/marrakesh -n marrakesh

# Multiple pods
kubectl logs -f -n marrakesh -l serving.knative.dev/service=marrakesh

# Previous revision
kubectl logs -f deployment/marrakesh --previous -n marrakesh

# Last 100 lines
kubectl logs --tail=100 deployment/marrakesh -n marrakesh

# Filter by trace ID
kubectl logs deployment/marrakesh -n marrakesh | grep trace-abc123
```

**Structured Log Query (jq)**:
```bash
# Find errors
kubectl logs deployment/marrakesh -n marrakesh | jq 'select(.level == "ERROR")'

# Find slow requests (> 1s)
kubectl logs deployment/marrakesh -n marrakesh | jq 'select(.http.duration_ms > 1000)'

# Group by path
kubectl logs deployment/marrakesh -n marrakesh | jq -r '.http.path' | sort | uniq -c
```

### Logging in Code

```rust
use tracing::{info, debug, warn, error, instrument};

#[instrument(skip_all, fields(user_id = user.id))]
pub async fn user_handler(user: User) -> Result<(), AppError> {
    info!("Processing user: {}", user.name);
    
    debug!("User data: {:?}", user.data);
    
    if user.age < 18 {
        warn!("User is underage: {}", user.id);
    }
    
    match process_user(user).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to process user {}: {}", user.id, e);
            Err(e)
        }
    }
}
```

### Log Levels

| Level | Usage | Production |
|-------|--------|-------------|
| TRACE | Very verbose | ❌ Too noisy |
| DEBUG | Detailed info | ❌ Too noisy |
| INFO | Normal operation | ✅ Recommended |
| WARN | Warning conditions | ✅ Recommended |
| ERROR | Errors that don't crash | ✅ Recommended |

**Configure via Environment**:
```bash
# Development
RUST_LOG=debug cargo run

# Production
export APP__TELEMETRY__LOG_LEVEL=info
export RUST_LOG=info
```

---

## Alerting

### Recommended Alert Rules

**High Error Rate**:
```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: marrakesh-alerts
spec:
  groups:
  - name: errors
    rules:
    - alert: HighErrorRate
      expr: sum(rate(http_requests_total{status=~"5.."}[5m])) > 0.05
      for: 5m
      annotations:
        summary: "High error rate on marrakesh"
        description: "Error rate is {{ $value }} errors/sec for the last 5 minutes"
      labels:
        severity: warning
```

**High Latency**:
```yaml
    - alert: HighLatency
      expr: histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le)) > 1
      for: 5m
      annotations:
        summary: "High latency on marrakesh"
        description: "95th percentile latency is {{ $value }}s"
      labels:
        severity: warning
```

**Scrape target down**:
```yaml
    - alert: TargetDown
      expr: up == 0
      for: 2m
      annotations:
        summary: "Prometheus target is down"
        description: "{{ $labels.job }} has been unreachable for 2 minutes"
      labels:
        severity: critical
```

### Slack Integration

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: slack-webhook
stringData:
  url: https://hooks.slack.com/services/...
---
apiVersion: notification.toolkit.fluxcd.io/v1beta1
kind: Provider
metadata:
  name: slack
spec:
  type: slack
  channel: alerts
  secretRef:
    name: slack-webhook
```

### PagerDuty Integration

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: pagerduty
stringData:
  integrationKey: YOUR_KEY
---
apiVersion: notification.toolkit.fluxcd.io/v1beta1
kind: Provider
metadata:
  name: pagerduty
spec:
  type: pagerduty
  secretRef:
    name: pagerduty
```

---

## Dashboards


The generated project ships a Grafana dashboard as a labeled ConfigMap
(`deploy/components/grafana-dashboards`) and provisions it automatically in the
local dev stack. See [`docs/GRAFANA_DASHBOARDS.md`](GRAFANA_DASHBOARDS.md) for
the sidecar contract, production wiring, and customization.


### Metric contract

The application exports these metrics from `/metrics`:

| Metric | Type | Labels |
|---|---|---|
| `http_requests_total` | counter | `service`, `status` |
| `http_request_duration_seconds` | histogram | `service`, `status` |
| `app_info` | counter | `service` |
| `kafka_events_published_total` | counter | `service`, `topic` |
| `kafka_events_failed_total` | counter | `service`, `topic`, `error_type` |
| `kafka_publish_latency_ms` | histogram | `service`, `topic` |


`service` is a global label whose value is the Rust crate name. CPU and memory
are collected by Kubernetes/cAdvisor (or kube-state-metrics), not by the
application.

**Example panels**:

1. **Request rate**: `sum(rate(http_requests_total{service=~"$service"}[$__rate_interval]))`
2. **Error ratio**: `sum(rate(http_requests_total{service=~"$service",status=~"5.."}[$__rate_interval])) / clamp_min(sum(rate(http_requests_total{service=~"$service"}[$__rate_interval])), 0.001)`
3. **p95 latency**: `histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket{service=~"$service"}[$__rate_interval])))`
4. **Targets up**: `sum(up{job=~"$job"})`

For CNPG PostgreSQL panels, use the official dashboard described in
[`docs/POSTGRES_MONITORING.md`](POSTGRES_MONITORING.md).

---

## Production Monitoring

### Key Metrics to Monitor

**Service Health**:
- Request success rate (> 99.9%)
- P95 latency (< 500ms)
- Error rate (< 0.1%)

**Resource Utilization**:
- CPU < 70%
- Memory < 80%
- Disk < 90%

**Dependencies**:
- Redis connection success rate
- S3 operation success rate
- OpenTelemetry exporter health

### SLIs and SLOs

**SLI (Service Level Indicator)**:
- Metric: `http_requests_total{status=~"2.."} / http_requests_total`
- Time window: 7 days rolling

**SLO (Service Level Objective)**:
- Target: 99.9% success rate
- Budget: 0.1% error budget per week

**Error Budget Calculation**:
- Requests/week: 1,000,000
- Allowed errors: 1,000
- Current errors: 50
- Remaining budget: 950 errors

### Runbooks

**High Error Rate**:
1. Check recent deployments
2. Review error logs
3. Check dependencies (Redis, S3)
4. Scale up if resource constrained
5. Rollback if needed

**High Latency**:
1. Check traces in Jaeger
2. Identify slow operation
3. Check resource utilization
4. Review database/Redis query performance
5. Scale up or optimize code

**Pod Not Ready**:
1. Describe pod: `kubectl describe pod <pod-name>`
2. Check pod logs
3. Verify ConfigMaps and Secrets
4. Check image pull errors
5. Restart deployment

---

## Next Steps

- **API Docs**: See `docs/API.md`
- **Development**: See `docs/DEVELOPMENT.md`
- **Troubleshooting**: See `docs/TROUBLESHOOTING.md`
- **Security**: See `docs/SECURITY.md`
