# Grafana Dashboards

The template can generate a Grafana dashboard for the first service and wire it
into the local development stack. The feature is opt-in:

```text
Generate a Grafana dashboard (labeled ConfigMap for a Grafana sidecar)? yes
```

The answer is stored as `feature_grafana_dashboards` in `cargo-generate.toml`
and defaults to `false`.

> **This project:** marrakesh deploys to the home Talos cluster. Grafana runs in
> the `monitoring` namespace as part of kube-prometheus-stack; its dashboard
> sidecar watches all namespaces (`NAMESPACE=ALL`) and discovers the labeled
> ConfigMap automatically. The local Kind Grafana described below is only for
> `make local-up`.

## What gets generated

```text
deploy/
├── components/grafana-dashboards/
│   ├── kustomization.yaml
│   └── dashboard.yaml          # ConfigMap + dashboard JSON
└── dev/observability/
    └── grafana.yaml            # dev-only Grafana + k8s-sidecar
```

The overlay includes the component in every environment:

```yaml
components:
  - ../../components/grafana-dashboards
```

The rendered object is a normal ConfigMap in the application namespace:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: marrakesh-dashboard
  labels:
    grafana_dashboard: "1"
data:
  marrakesh-overview.json: |-
    { ... }
```

The label is the contract. Any Grafana dashboard sidecar configured with
`LABEL=grafana_dashboard` and the right namespace watch discovers it. The
`grafana_folder: Apps` annotation is honored when the sidecar sets
`FOLDER_ANNOTATION=grafana_folder` (the kube-prometheus-stack default); the dev
stack's provider folder is `Apps` as well.

## Local development

`make local-up` deploys `deploy/dev/observability`, which includes Grafana when
the feature is on. `make dev-forward` forwards it:

- Grafana: http://localhost:3000 (`admin` / `admin`)
- Prometheus: http://localhost:9090
- Jaeger: http://localhost:16686

The dev Grafana uses the same k8s-sidecar contract as kube-prometheus-stack, so
a dashboard that appears locally will also be discovered by a production
sidecar with the same label.

`make dev-observability` redeploys only the stack.

## Production wiring

### kube-prometheus-stack (sidecar)

Enable the dashboard sidecar and make it watch the application namespace (or all
namespaces):

```yaml
grafana:
  sidecar:
    dashboards:
      enabled: true
      label: grafana_dashboard
      labelValue: "1"
      searchNamespace: ALL   # or the application namespace
```

`searchNamespace: ALL` is the simplest option when the ConfigMap lives in the
application namespace and Grafana lives in `monitoring`.

### Grafana Operator

The generated object is a ConfigMap, not a `GrafanaDashboard` CR. If you use the
Grafana Operator:

- keep a ConfigMap-watching sidecar alongside the operator, or
- convert `deploy/components/grafana-dashboards/dashboard.yaml` to a
  `GrafanaDashboard` CR whose `spec.json` is the same dashboard JSON.

Keep the JSON as the source of truth either way.

### Managed Grafana (Grafana Cloud, etc.)

A ConfigMap is inert without a cluster-side Grafana. Push the dashboard JSON
through the Grafana API from CI or a Make target:

```bash
kubectl get configmap marrakesh-dashboard \
  -o jsonpath='{.data.marrakesh-overview\.json}' \
  | jq '{dashboard: ., overwrite: true}' \
  | curl -fsS -X POST "$GRAFANA_URL/api/dashboards/db" \
      -H "Authorization: Bearer $GRAFANA_TOKEN" \
      -H "Content-Type: application/json" \
      --data-binary @-
```

Keep the token in CI secrets, never in the repository.

## Dashboard contents

The generated dashboard uses the `service` label for application panels and a
`Scrape job` variable for scrape health. Panels are feature-gated at generation
time, so disabled features do not leave empty panels:

| Panel | Query source |
|---|---|
| Requests / second | `http_requests_total` |
| 5xx error ratio | `http_requests_total{status=~"5.."}` |
| p50 / p95 / p99 latency | `http_request_duration_seconds_bucket` |
| Request rate by status | `http_requests_total` |
| Scrape targets up | `up` |
| Kafka events published / failures / p99 | `kafka_events_*`, `kafka_publish_latency_ms` |
| Bucket notifications | `bucket_notifications_received_total` |
| Active SSE connections, churn, heartbeats | `stream_*` (Deployment workload only) |
| Scheduled events | `scheduled_events_received_total` (Knative + cron only) |
| Application info | `app_info` |

The full metric contract:

| Metric | Type | Labels | Emitted when |
|---|---|---|---|
| `http_requests_total` | counter | `service`, `status` | always |
| `http_request_duration_seconds` | histogram | `service`, `status` | always |
| `app_info` | counter | `service` | always |
| `kafka_events_published_total` | counter | `service`, `topic` | `feature_kafka` |
| `kafka_events_failed_total` | counter | `service`, `topic`, `error_type` | `feature_kafka` |
| `kafka_publish_latency_ms` | histogram | `service`, `topic` (and `error=true` on failures) | `feature_kafka` |
| `scheduled_events_received_total` | counter | `service` | `feature_cron` (Knative) |
| `bucket_notifications_received_total` | counter | `service` | bucket notifications |
| `stream_connections_total` | counter | `service` | Deployment workload |
| `stream_connections_active` | gauge | `service` | Deployment workload |
| `stream_heartbeats_sent_total` | counter | `service` | Deployment workload |

`service` is a global label added by `crates/service-core`; its value is the
Rust crate name (`env!("CARGO_PKG_NAME")`), so it uses underscores even though
the Kubernetes name uses hyphens. Keep that in mind when writing saved queries.

The `status` label on HTTP metrics is set by the metrics middleware in
`src/routes.rs` (and `service-template/src/routes.rs` for services added with
`make add-service`). The latency histogram is a true Prometheus histogram
because `init_metrics` configures buckets explicitly.

## Customizing

Edit `deploy/components/grafana-dashboards/dashboard.yaml`:

1. Keep the dashboard `uid` stable (`<project>-overview`, truncated to Grafana's
   40-character limit for long project names) so links and the dev
   home-dashboard path keep working.
2. Prefer the `${DS_PROMETHEUS}` datasource variable and the `$service` template
   variable over hard-coded names.
3. Gate new panels with Liquid so they only render for features the project
   actually has.
4. Update the CI assertions in
   `.github/workflows/template-generate-and-validate.yaml` when you add a panel
   that must always exist.

The dev Grafana allows UI edits, but the ConfigMap is the source of truth.
Export a changed dashboard and commit the JSON back into the component.

### Multiple services

`make add-service` does not generate a second dashboard. Each added service
owns `services/<service>/deploy/`; copy the component into that subtree, rename
the ConfigMap to `<service>-dashboard`, change the dashboard `uid`, and register
the directory in the per-environment overlay. Keep one dashboard per service so
the `service` variable can distinguish them.

## Validation

Render the overlay and parse the dashboard JSON before committing:

```bash
kubectl kustomize deploy/overlays/dev > /tmp/dev-overlay.yaml
yq eval 'select(.kind == "ConfigMap" and .metadata.labels.grafana_dashboard == "1") | .data' \
  /tmp/dev-overlay.yaml > /tmp/dashboard-data.yaml
yq eval 'to_entries | .[0].value' /tmp/dashboard-data.yaml | jq empty
```

The template CI renders the all-features project and asserts the dashboard
title, UID, feature-gated panels, and the dev Grafana sidecar wiring.

## Limitations

- The dev stack only provisions a dashboard provider; it does not install
  Grafana in staging or production. Use the sidecar, operator, or API path
  above.
- No CNPG panels are generated. PostgreSQL metrics come from the operator;
  see `docs/POSTGRES_MONITORING.md` for the official dashboard.
- No Loki/log panels are generated; the template does not ship Loki.
- The dev Prometheus scrapes the app twice: once through the static
  `rust-service` job and once through the annotation-based `kubernetes-pods`
  job (the chart sets `prometheus.io/scrape`). Use the **Scrape job** variable
  to select one of them if totals look doubled; production usually has a single
  ServiceMonitor/PodMonitor.
- The dev Prometheus scrape job is static (`rust-service`), so `job` values in
  staging/production may differ. Application panels use `service` instead.
- ConfigMaps are capped at 1 MiB. A dashboard of this size is far below that;
  if you embed many large dashboards, split them into separate keys/ConfigMaps.
