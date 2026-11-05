# Deployment Guide

Complete guide for deploying marrakesh to Kubernetes using Knative Serving and FluxCD GitOps.

> **About Template Variables**: This is a template file that uses Liquid syntax for variables like `marrakesh`. During project generation with `cargo-generate`, these placeholders are replaced with your actual project values. The generated documentation will contain your specific project name instead of the template variable.

## About Project Naming

### Template Variable: `project_name`

When generating a project from this template, you'll be prompted for a `project_name`. This name is automatically normalized to follow platform-specific conventions:

- **Kubernetes Resources** (Knative Services, ConfigMaps, Secrets): Uses **kebab-case** (hyphens)
  - Example: `my-service`, `user-api`, `payment-processor`
  - Kubernetes DNS names require hyphens, not underscores
  
- **Rust Crate** (Cargo.toml): Uses **snake_case** (underscores)
  - Example: `my_service`, `user_api`, `payment_processor`
  - Rust convention for package names

- **Docker Images**: Uses **kebab-case** (hyphens) to match Kubernetes names
  - Example: `ghcr.io/org/my-service:latest`

**What You Input**: When running `cargo generate`, you can provide `project_name` in any format:
- `my-service` (hyphens)
- `my_service` (underscores)  
- `myservice` (single word)

**Template Normalization**: The template automatically converts your input:
- `marrakesh` → Forces kebab-case for Kubernetes/Docker
- `marrakesh` → Automatically converted to snake_case for Rust

**Why This Matters**: Knative Services **cannot** contain underscores in their names. Using underscores will cause `ImagePullBackOff` errors because the image tag won't match the service name. The template handles this normalization automatically.

> **For Detailed Information**: Naming is normalized at generation time: Rust uses underscores, Kubernetes and Docker use hyphens. See the naming checks in `.github/workflows/naming-convention-tests.yaml` in the template repository.

## Table of Contents

- [About Project Naming](#about-project-naming)
- [Prerequisites](#prerequisites)
- [Local Testing](#local-testing)
- [Manual Kubernetes Deployment](#manual-kubernetes-deployment)
- [FluxCD GitOps Deployment](#fluxcd-gitops-deployment)
- [Environment-Specific Deployment](#environment-specific-deployment)
- [Rollback Procedures](#rollback-procedures)
- [Traffic Splitting](#traffic-splitting)
- [Monitoring Deployments](#monitoring-deployments)

---

## Prerequisites

### Required Software

| Software | Version | Purpose |
|----------|---------|---------|
| kubectl | 1.24+ | Kubernetes CLI |
| Knative Serving | 1.12+ | Serverless platform |
| FluxCD CLI | 2.0+ | GitOps tool |
| Docker | 20.10+ | Container build |
| helm | 3.x+ | Package manager (optional) |

### Kubernetes Cluster

**Recommended for Production**:
- EKS (AWS), GKE (Google), AKS (Azure)
- 3+ nodes for high availability
- 8+ GB RAM per node
- 50+ GB storage

**For Development/Testing**:
- Kind (Kubernetes in Docker)
- Minikube
- Local development cluster

### Install Knative Serving

```bash
# Install Knative Serving with Kourier networking
kubectl apply -f https://github.com/knative/serving/releases/download/knative-v1.12.0/serving-crds.yaml
kubectl apply -f https://github.com/knative/serving/releases/download/knative-v1.12.0/serving-core.yaml
kubectl apply -f https://github.com/knative/net-kourier/releases/download/knative-v1.12.0/kourier.yaml

# Configure Kourier as default ingress
kubectl patch configmap/config-network \
  --namespace knative-serving \
  --type merge \
  --patch '{"data":{"ingress.class":"kourier.ingress.networking.knative.dev"}}'

# Verify installation
kubectl get pods -n knative-serving
```

### Install FluxCD

```bash
# Install FluxCD CLI
curl -s https://fluxcd.io/install.sh | sudo bash

# Verify installation
flux --version

# Bootstrap FluxCD in your cluster
flux bootstrap github \
  --owner=your-org \
  --repo=your-repo \
  --personal \
  --path=clusters/production
```

---

## Local Testing

Before deploying to Kubernetes, test locally:

### 1. Build Docker Image

```bash
# Build image
docker build -t marrakesh:latest .

# Tag for registry
docker tag marrakesh:latest ghcr.io/enchantednatures/marrakesh:v1.0.0

# Push to registry
docker push ghcr.io/enchantednatures/marrakesh:v1.0.0
```

### 2. Test with Kind Development Environment

```bash
# Start development environment
make local-up

# Verify health
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready

# Test API

curl -X POST http://localhost:8080/api/upload \
  -H "Content-Type: application/json" \
  -d '{"key":"test.txt","data":"aGVsbG8="}'


# View logs
make dev-logs
```

### 3. Run Integration Tests

```bash
# Start development environment
make local-up

# Run tests
cargo test -- --ignored --nocapture
```

---

## Manual Kubernetes Deployment

### Create Namespace

```bash
kubectl create namespace marrakesh
```

### Create Secrets

```bash
# Redis connection
kubectl create secret generic marrakesh-secrets \
  --from-literal=redis-url='redis://redis:6379' \
  -n marrakesh


# S3 credentials
kubectl create secret generic marrakesh-s3 \
  --from-literal=aws-access-key-id='your-access-key' \
  --from-literal=aws-secret-access-key='your-secret-key' \
  -n marrakesh

```

### Deploy Base Service

```bash
# Apply the base HelmRelease (Flux renders the Knative Service via deploy/chart)
kubectl apply -k deploy/base -n marrakesh

# Get service URL
kubectl get ksvc marrakesh -n marrakesh

# Test
export URL=$(kubectl get ksvc marrakesh -n marrakesh -o jsonpath='{.status.url}')
curl https://$URL/health/live
```

---

## FluxCD GitOps Deployment

GitOps manages infrastructure state via Git. FluxCD syncs manifests to the cluster.

### Repository Structure

```
your-repo/
├── clusters/
│   └── production/
│       ├── flux-system/
│       │   ├── gotk-components.yaml
│       │   └── kustomization.yaml
│       └── marrakesh/
│           ├── kustomization.yaml
│           └── apps.yaml
├── deploy/
│   ├── base/
│   ├── overlays/
│   └── flux/
│       ├── git-repository.yaml
│       ├── kustomization.yaml
│       └── image-repository.yaml
```

### One-command deploy (`make deploy`)

After generating and pushing the repository:

```bash
# Private repositories: create the flux-system SSH deploy-key Secret once
make git-auth

# Commit the working tree, push, and (re)apply the Flux bootstrap for an env
make deploy dev
```

`make deploy <env>` (env: dev, staging, prod) commits any pending changes, pushes, then applies the GitRepository + Kustomizations for that environment — the apply is a no-op after the first run, so day-to-day it is "commit + push + reconcile". Flux then reconciles `deploy/overlays/<env>`.

Generation knobs that shape this flow:

| Prompt | Effect |
|---|---|
| `git_auth_secret` | GitRepository pulls over SSH with a `secretRef`; empty keeps public HTTPS |
| `image_pull_secret` | Adds `imagePullSecrets` to the app image; create the encrypted Secret from `deploy/overlays/dev/image-pull-secret.sops.yaml.example` |
| `custom_domain` | Adds a `ClusterDomainClaim` + `DomainMapping` (dev overlay) for the given hostname |
| `network_policy_allow_all` | Adds an allow-all NetworkPolicy to the dev namespace |
| `target_namespace` | Namespace for every overlay (each renders a Namespace object when it is not `default`) |
| `postgres_storage_class` | Sets `spec.storage.storageClass` on the CNPG `Cluster` (default `standard`) |
| `postgres_instances_{dev,staging,prod}` | Replica count per environment (1/2/3) |
| `postgres_pooler` | Adds the PgBouncer `Pooler` and routes the app through `APP__POSTGRES__HOST` |

The Kubeconfig used by `make bootstrap`/`make deploy` is `$KUBECONFIG` when set, otherwise `CLUSTER_KUBECONFIG` (defaults to `~/.kube/config`). The local Kind cluster is `make local-up` / `make local-down`; SOPS configuration starts from the gitignored `.sops.yaml` (copy `.sops.yaml.example`); no Secret is SOPS-encrypted in the generated tree.

### Step 1: Create GitRepository

```bash
# Create GitRepository source
kubectl apply -f deploy/flux/git-repository.yaml -n marrakesh
```

`deploy/flux/git-repository.yaml`:
```yaml
apiVersion: source.toolkit.fluxcd.io/v1beta2
kind: GitRepository
metadata:
  name: marrakesh
  namespace: marrakesh
spec:
  interval: 1m
  url: https://github.com/enchantednatures/marrakesh
  ref:
    branch: main
  secretRef:
    name: github-token
```

### Step 2: Create Kustomization

```bash
# Bootstrap Flux to track this repo + deploy to production
make bootstrap production
```

Or apply manually per environment:

```bash
kubectl apply -f deploy/flux/kustomization-prod.yaml
```

`deploy/flux/kustomization-prod.yaml`:
```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1beta2
kind: Kustomization
metadata:
  name: marrakesh
  namespace: marrakesh
spec:
  interval: 5m
  sourceRef:
    kind: GitRepository
    name: marrakesh
  path: ./deploy/overlays/prod
  prune: true
  timeout: 2m
```

### Step 3: Enable Image Automation

Automatically update image references when new images are pushed:

```yaml
# deploy/flux/image-repository.yaml
apiVersion: image.toolkit.fluxcd.io/v1beta2
kind: ImageRepository
metadata:
  name: marrakesh
  namespace: marrakesh
spec:
   image: ghcr.io/enchantednatures/marrakesh
  interval: 5m
  secretRef:
    name: registry-credentials

# deploy/flux/image-policy.yaml
apiVersion: image.toolkit.fluxcd.io/v1beta2
kind: ImagePolicy
metadata:
  name: marrakesh
  namespace: marrakesh
spec:
  imageRepositoryRef:
    name: marrakesh
  policy:
    semver:
      range: ">=1.0.0"

# Alternative: Use the 'release' tag for stable deployments
# This tag is automatically applied by the release workflow
# deploy/flux/image-policy-release.yaml
apiVersion: image.toolkit.fluxcd.io/v1beta2
kind: ImagePolicy
metadata:
  name: marrakesh-release
  namespace: marrakesh
spec:
  imageRepositoryRef:
    name: marrakesh
  filterTags:
    pattern: '^release$'
  policy:
    alphabetical:
      order: asc
```

**Image Policy Options:**

1. **SHA-based (Continuous Deployment)**: Automatically deploy every commit to main
   - Pattern: `sha-[a-f0-9]{7}`
   - Use case: Dev/staging environments
   - Pros: Immediate feedback, automated
   - Cons: May deploy broken builds

2. **Release tag (Stable Deployment)**: Deploy only when you create a release
   - Pattern: `release`
   - Use case: Production environments
   - Pros: Controlled, stable, tested
   - Cons: Manual release process

3. **SemVer (Version-based)**: Deploy specific semantic versions
   - Pattern: Semver range (e.g., `>=1.0.0`)
   - Use case: Production with version control
   - Pros: Explicit versioning, rollback-friendly
   - Cons: Requires version management

### Step 4: Apply Deployment

```bash
# Create FluxCD resources
kubectl apply -k deploy/flux

# Sync FluxCD
flux reconcile source git flux-system

# Verify sync
flux get kustomizations --watch
```

---



## Environment-Specific Deployment

### Development

```bash
# Deploy to development namespace
kubectl apply -k deploy/overlays/dev -n marrakesh-dev

# Configuration:
# - Min 1 replica (no scale-to-zero)
# - Debug logging
# - RustFS for storage
# - Lower resource limits
```

`deploy/overlays/dev/kustomization.yaml`:
```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: marrakesh-dev
resources:
  - ../../base

patchesStrategicMerge:
- |-
  apiVersion: serving.knative.dev/v1
  kind: Service
  metadata:
    name: marrakesh
  spec:
    template:
      metadata:
        annotations:
          autoscaling.knative.dev/minScale: "1"
          autoscaling.knative.dev/maxScale: "10"
          autoscaling.knative.dev/target: "10"
      spec:
        containerConcurrency: 10
```

### Staging

```bash
# Deploy to staging namespace
kubectl apply -k deploy/overlays/staging -n marrakesh-staging

# Configuration:
# - 2-20 replicas
# - Info logging
# - AWS S3 storage
# - Higher resource limits
```

`deploy/overlays/staging/kustomization.yaml`:
```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: marrakesh-staging
resources:
  - ../../base

patches:
- patch: |-
    apiVersion: serving.knative.dev/v1
    kind: Service
    metadata:
      name: marrakesh
    spec:
      template:
        metadata:
          annotations:
            autoscaling.knative.dev/minScale: "2"
            autoscaling.knative.dev/maxScale: "20"
            autoscaling.knative.dev/targetUtilizationPercentage: "70"
        spec:
          template:
            spec:
              containers:
              - name: user-container
                env:
                - name: APP__TELEMETRY__LOG_LEVEL
                  value: "info"
                - name: APP_ENV
                  value: "staging"
  target:
    kind: Service
    name: marrakesh
```

### Production

```bash
# Deploy to production namespace
kubectl apply -k deploy/overlays/prod -n marrakesh-prod

# Configuration:
# - 5-100 replicas
# - Warn logging
# - AWS S3 with high availability
# - Production secrets via External Secrets
```

`deploy/overlays/prod/kustomization.yaml`:
```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: marrakesh-prod
resources:
  - ../../base

patches:
- patch: |-
    apiVersion: serving.knative.dev/v1
    kind: Service
    metadata:
      name: marrakesh
    spec:
      template:
        metadata:
          annotations:
            autoscaling.knative.dev/minScale: "5"
            autoscaling.knative.dev/maxScale: "100"
            autoscaling.knative.dev/targetUtilizationPercentage: "80"
        spec:
          template:
            spec:
              containers:
              - name: user-container
                env:
                - name: APP__TELEMETRY__LOG_LEVEL
                  value: "warn"
                - name: APP_ENV
                  value: "production"
                resources:
                  limits:
                    memory: "1Gi"
                    cpu: "1000m"
                  requests:
                    memory: "512Mi"
                    cpu: "500m"
  target:
    kind: Service
    name: marrakesh

secretGenerator:
- name: marrakesh-secrets
  envs:
  - .env.production
```

---

## Rollback Procedures

### Option 1: Manual Revision Rollback

```bash
# List revisions
kubectl revisions list -n marrakesh

# Rollback to previous revision
kubectl rollout undo service/marrakesh -n marrakesh

# Rollback to specific revision
kubectl set traffic marrakesh \
  --revision=marrakesh-00001 \
  --percent=100 \
  -n marrakesh
```

### Option 2: Git Rollback (GitOps)

```bash
# Revert commit
git revert <commit-hash>

# Push to trigger FluxCD sync
git push origin main

# Verify new revision deployed
flux get kustomizations -n marrakesh
```

### Option 3: Canary Rollback

If using canary deployment:

```bash
# Shift all traffic back to stable
kubectl set traffic marrakesh \
  --revision=marrakesh-stable \
  --percent=100 \
  -n marrakesh
```

---

## Traffic Splitting

Knative supports traffic splitting between revisions for canary deployments.

### Example: 10% Traffic to New Version

```bash
# Deploy new version
kubectl set image service/marrakesh \
  user-container=ghcr.io/enchantednatures/marrakesh:v2.0.0 \
  -n marrakesh

# Split traffic: 90% v1, 10% v2
kubectl set traffic marrakesh \
  --revision=marrakesh-00001=90 \
  --revision=marrakesh-00002=10 \
  -n marrakesh

# Monitor traffic split
kubectl describe ksvc marrakesh -n marrakesh
```

### Gradual Rollout

```bash
# 10%
kubectl set traffic marrakesh \
  --revision=marrakesh-00001=90 \
  --revision=marrakesh-00002=10

# Wait and monitor (e.g., check error rates in Prometheus)
# Then increase to 25%
kubectl set traffic marrakesh \
  --revision=marrakesh-00001=75 \
  --revision=marrakesh-00002=25

# 50%
kubectl set traffic marrakesh \
  --revision=marrakesh-00001=50 \
  --revision=marrakesh-00002=50

# 100% to new version
kubectl set traffic marrakesh \
  --revision=marrakesh-00002=100
```

### Blue-Green Deployment

```bash
# Create blue (current) revision
kubectl set traffic marrakesh \
  --revision=marrakesh-00001=100

# Deploy green (new) revision
kubectl apply -f deploy/overlays/prod -n marrakesh

# Shift all traffic to green
kubectl set traffic marrakesh \
  --revision=marrakesh-00002=100

# Keep blue for rollback
# Delete blue later after verification
```

---

## Monitoring Deployments

### Check Service Status

```bash
# Get service URL
kubectl get ksvc marrakesh -n marrakesh

# Get service details
kubectl describe ksvc marrakesh -n marrakesh

# Get pods
kubectl get pods -n marrakesh -l serving.knative.dev/service=marrakesh

# Get revisions
kubectl revisions list -n marrakesh
```

### Check FluxCD Status

```bash
# Get FluxCD sources
flux get sources git -n marrakesh

# Get FluxCD kustomizations
flux get kustomizations -n marrakesh

# Sync manually
flux reconcile kustomization marrakesh -n marrakesh

# Get conditions
flux get kustomizations -n marrakesh --watch
```

### View Logs

```bash
# Service logs
kubectl logs -f -n marrakesh deployment/marrakesh

# Specific pod
kubectl logs -f -n marrakesh <pod-name>

# Previous revision logs
kubectl logs -f -n marrakesh deployment/marrakesh --previous

# Knative serving logs
kubectl logs -n knative-serving deployment/controller
kubectl logs -n knative-serving deployment/autoscaler
```

---

## CI/CD Pipeline

### Continuous Integration (CI)

The CI workflow runs on every push to main and on pull requests:

```yaml
# .github/workflows/ci.yaml
name: CI/CD Pipeline

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Run Clippy
        run: cargo clippy --all-targets -- -D warnings
  
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Run tests
        run: cargo test --all-features
  
  build:
    needs: [lint, test]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Build and push Docker image
        uses: docker/build-push-action@v7
        with:
          push: true
          tags: |
            ${{ env.REGISTRY }}/${{ github.repository }}:sha-${{ github.sha }}
            ${{ env.REGISTRY }}/${{ github.repository }}:latest
```

### Release Workflow

The release workflow is triggered when you push a Git tag (e.g., `v1.0.0`):

```yaml
# .github/workflows/release.yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  create-release:
    runs-on: ubuntu-latest
    steps:
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref }}
          name: Release ${{ github.ref }}
  
  build-release:
    needs: create-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Build and push release image
        uses: docker/build-push-action@v7
        with:
          push: true
          tags: |
            ${{ env.REGISTRY }}/${{ github.repository }}:${{ github.ref_name }}
            ${{ env.REGISTRY }}/${{ github.repository }}:release
          build-args: |
            RUST_BASE_IMAGE_TAG=release
```

**Creating a Release:**

```bash
# Tag your commit
git tag -a v1.0.0 -m "Release version 1.0.0"

# Push the tag to trigger the release workflow
git push origin v1.0.0

# The release workflow will:
# 1. Create a GitHub release
# 2. Build a Docker image with Rust 'release' base image if available
# 3. Tag the image with version (1.0.0) and 'release'
# 4. Run security scans
```

**Base Image Policy:**

The release workflow uses a policy-driven approach for selecting Rust base images:

- **Development builds**: Use versioned Rust image (e.g., `rust:1.92-slim`)
- **Release builds**: Prefer `rust:release` tag if available, otherwise fall back to versioned tag
- **Benefits**: 
  - Stable, tested Rust versions for releases
  - Reproducible builds across environments
  - Security patches applied to release images

### FluxCD Integration

Once images are pushed, FluxCD can automatically deploy them based on your ImagePolicy:

```bash
# Deploy using the 'release' tag (recommended for production)
kubectl apply -f - <<EOF
apiVersion: image.toolkit.fluxcd.io/v1beta2
kind: ImagePolicy
metadata:
  name: marrakesh-release
  namespace: flux-system
spec:
  imageRepositoryRef:
    name: marrakesh
  filterTags:
    pattern: '^release$'
EOF

# Deploy using semver (alternative for version-controlled deployments)
kubectl apply -f - <<EOF
apiVersion: image.toolkit.fluxcd.io/v1beta2
kind: ImagePolicy
metadata:
  name: marrakesh-semver
  namespace: flux-system
spec:
  imageRepositoryRef:
    name: marrakesh
  policy:
    semver:
      range: ">=1.0.0"
EOF
```

---

## Troubleshooting Deployments

### Service Not Ready

```bash
# Describe service
kubectl describe ksvc marrakesh -n marrakesh

# Check conditions
kubectl get ksvc marrakesh -n marrakesh -o jsonpath='{.status.conditions[*]}' | jq

# Common issues:
# - ImagePullBackOff: Check image registry credentials
# - CrashLoopBackOff: Check application logs
# - ConfigMissing: Check ConfigMaps and Secrets exist
```

### Pods Not Starting

```bash
# Check pod status
kubectl get pods -n marrakesh

# Get pod events
kubectl describe pod <pod-name> -n marrakesh

# Check pod logs
kubectl logs <pod-name> -n marrakesh

# Common issues:
# - OOMKilled: Increase memory limits
# - FailedScheduling: Check node resources
# - ContainerCreating: Check image pull progress
```

### FluxCD Not Syncing

```bash
# Get FluxCD logs
kubectl logs -n flux-system deployment/source-controller
kubectl logs -n flux-system deployment/kustomize-controller

# Check GitRepository status
kubectl get gitrepository -n marrakesh

# Check Kustomization status
kubectl get kustomization -n marrakesh

# Reconcile manually
flux reconcile source git flux-system
flux reconcile kustomization marrakesh -n marrakesh
```

---

## Security Best Practices

1. **Use Secrets for Credentials**:
   ```yaml
   env:
   - name: AWS_ACCESS_KEY_ID
     valueFrom:
       secretKeyRef:
         name: marrakesh-secrets
         key: aws-access-key-id
   ```

2. **Enable RBAC**:
   ```yaml
   apiVersion: v1
   kind: ServiceAccount
   metadata:
     name: marrakesh
   ---
   apiVersion: rbac.authorization.k8s.io/v1
   kind: Role
   rules:
   - apiGroups: [""]
     resources: ["configmaps", "secrets"]
     verbs: ["get", "list"]
   ```

3. **Network Policies**:
   ```yaml
   apiVersion: networking.k8s.io/v1
   kind: NetworkPolicy
   metadata:
     name: marrakesh
   spec:
     podSelector:
       matchLabels:
         serving.knative.dev/service: marrakesh
     policyTypes:
     - Ingress
     - Egress
   ```

4. **Image Vulnerability Scanning**:
   ```yaml
   - name: Run Trivy
     run: trivy image --severity HIGH,CRITICAL ${{ secrets.REGISTRY }}/marrakesh:${{ github.sha }}
   ```

See `docs/SECURITY.md` for complete security guidance.

---

## Next Steps

- **Configuration**: See `docs/CONFIGURATION.md` for environment-specific config
- **Monitoring**: See `docs/MONITORING.md` for observability
- **Troubleshooting**: See `docs/TROUBLESHOOTING.md` for deployment issues
- **Security**: See `docs/SECURITY.md` for hardening
