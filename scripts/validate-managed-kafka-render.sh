#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CHART="${ROOT}/deploy/chart"
OVERLAY="${ROOT}/deploy/overlays/dev"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_value() {
  local description="$1" expected="$2" actual="$3"
  if [[ "${actual}" != "${expected}" ]]; then
    fail "${description}: expected '${expected}', got '${actual:-<empty>}'"
  fi
  echo "PASS: ${description}"
}

echo "Building chart dependencies"
helm dependency update "${CHART}" >/dev/null

echo "Rendering the dev overlay values"
kubectl kustomize "${OVERLAY}" > "${TMP}/dev.yaml"
yq 'select(.kind == "HelmRelease") | .spec.values' "${TMP}/dev.yaml" > "${TMP}/values.yaml"

echo "Linting the chart with managed Kafka values"
helm lint "${CHART}" -n marrakesh -f "${TMP}/values.yaml" >/dev/null

echo "Rendering the managed Kafka release"
helm template marrakesh "${CHART}" -n marrakesh -f "${TMP}/values.yaml" > "${TMP}/render.yaml"

require_value "managed Redpanda CR" "marrakesh-kafka" \
  "$(yq 'select(.kind == "Redpanda") | .metadata.name' "${TMP}/render.yaml" | head -1)"
require_value "managed Topic CR" "events" \
  "$(yq 'select(.kind == "Topic") | .metadata.name' "${TMP}/render.yaml" | head -1)"
require_value "Kafka Broker CR" "marrakesh-kafka-broker" \
  "$(yq 'select(.kind == "Broker") | .metadata.name' "${TMP}/render.yaml" | head -1)"
require_value "Kafka Trigger CR" "marrakesh-events-trigger" \
  "$(yq 'select(.kind == "Trigger") | .metadata.name' "${TMP}/render.yaml" | head -1)"
require_value "KafkaSource CR" "marrakesh-events-kafka-source" \
  "$(yq 'select(.kind == "KafkaSource") | .metadata.name' "${TMP}/render.yaml" | head -1)"
require_value "broker ConfigMap" "marrakesh-kafka-broker-config" \
  "$(yq 'select(.kind == "ConfigMap" and .metadata.name == "marrakesh-kafka-broker-config") | .metadata.name' "${TMP}/render.yaml" | head -1)"

require_value "KafkaSource derived bootstrap" "marrakesh-kafka.marrakesh.svc.cluster.local:9093" \
  "$(yq 'select(.kind == "KafkaSource") | .spec.bootstrapServers[0]' "${TMP}/render.yaml" | head -1)"
require_value "KafkaSource sinks to the Broker" "Broker" \
  "$(yq 'select(.kind == "KafkaSource") | .spec.sink.ref.kind' "${TMP}/render.yaml" | head -1)"
require_value "KafkaSource broker name" "marrakesh-kafka-broker" \
  "$(yq 'select(.kind == "KafkaSource") | .spec.sink.ref.name' "${TMP}/render.yaml" | head -1)"
require_value "Trigger subscriber" "marrakesh" \
  "$(yq 'select(.kind == "Trigger") | .spec.subscriber.ref.name' "${TMP}/render.yaml" | head -1)"
require_value "broker bootstrap" "marrakesh-kafka.marrakesh.svc.cluster.local:9093" \
  "$(yq 'select(.kind == "ConfigMap" and .metadata.name == "marrakesh-kafka-broker-config") | .data["bootstrap.servers"]' "${TMP}/render.yaml" | head -1)"
require_value "app managed Kafka URL" "marrakesh-kafka:9093" \
  "$(yq 'select(.kind == "Service" and .metadata.name == "marrakesh") | .spec.template.spec.containers[0].env[] | select(.name == "APP__KAFKA__BROKER_URL") | .value' "${TMP}/render.yaml" | head -1)"

require_value "tuning init disabled" "false" \
  "$(yq 'select(.kind == "Redpanda") | .spec.clusterSpec.statefulset.initContainers.tuning.tune_aio_events' "${TMP}/render.yaml" | head -1)"
require_value "sidecar memory limit" "512Mi" \
  "$(yq 'select(.kind == "Redpanda") | .spec.clusterSpec.statefulset.podTemplate.spec.containers[] | select(.name == "sidecar") | .resources.limits.memory' "${TMP}/render.yaml" | head -1)"

require_value "managed cluster enabled" "true" \
  "$(yq 'select(.kind == "HelmRelease") | .spec.values.kafka.cluster.enabled' "${TMP}/dev.yaml")"
require_value "managed broker enabled" "true" \
  "$(yq 'select(.kind == "HelmRelease") | .spec.values.kafka.broker.enabled' "${TMP}/dev.yaml")"

if [[ -n "$(yq 'select(.kind == "HelmRelease") | .spec.values.kafka.sources.events.bootstrapServers // ""' "${TMP}/dev.yaml")" ]]; then
  fail "dev overlay still sets kafka.sources.events.bootstrapServers"
fi
if [[ -n "$(yq 'select(.kind == "HelmRelease") | .spec.values.kafka.sources.events.sink // ""' "${TMP}/dev.yaml")" ]]; then
  fail "dev overlay still sets kafka.sources.events.sink"
fi

if grep -qE 'redpanda-cluster\.redpanda|kafka\.kafka\.svc\.cluster\.local' "${TMP}/render.yaml"; then
  fail "rendered manifests still reference an external Kafka cluster"
fi

echo "managed Kafka render: OK"
