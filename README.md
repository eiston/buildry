# Buildry home platform

Reproducible local Kubernetes and GitOps platform, initially hosted in WSL2.
The Kubernetes resources are kept host-independent so the cluster can later move
to a Linux VM.

## Current stack

- Ubuntu 24.04 on WSL2 with systemd
- k3s `v1.36.2+k3s1`
- Argo CD `v3.4.5`
- Root GitOps application synchronized from `eiston/buildry`

## Bootstrap

From Ubuntu in WSL:

```bash
cd ~/git/buildry
make bootstrap
make status
```

The bootstrap is idempotent. It installs k3s as a systemd service, creates a
user-owned kubeconfig at `~/.kube/config`, and installs Argo CD from its pinned
upstream manifest.

For direct `kubectl` commands outside `make`, set:

```bash
export KUBECONFIG="$HOME/.kube/config"
```

Open Argo CD locally:

```bash
export KUBECONFIG="$HOME/.kube/config"
kubectl port-forward --address 127.0.0.1 service/argocd-server \
  -n argocd 8080:443
```

Then visit `https://localhost:8080`, sign in as `admin`, and obtain the initial
password with:

```bash
make argocd-password
```

Do not expose the Argo CD interface publicly. The next stage will add the root
application workloads and Cloudflare Tunnel.

## GitOps layout

`bootstrap/root-application.yaml` is the only cluster resource applied directly
after Argo CD is installed. It tells Argo CD to reconcile `clusters/home` from
the `main` branch. Everything placed under that directory is subsequently
managed by Argo CD.

## Test application

After the test application has synchronized, access it without exposing a
public port:

```bash
export KUBECONFIG="$HOME/.kube/config"
kubectl port-forward -n hello service/hello 8081:80
```

Visit `http://localhost:8081`. Cloudflare Tunnel will later route a custom
hostname to this service without requiring a stable home IP.

## Cloudflare Tunnel

The Cloudflare account resources are declared under `infrastructure/cloudflare`.
The Kubernetes connector is declared under `clusters/home/platform/cloudflared`.
The connector token is sensitive and is intentionally not committed.

Prerequisites:

- OpenTofu available as `tofu`
- `CLOUDFLARE_API_TOKEN` exported with Tunnel Write and DNS Edit permissions
- A private R2 bucket named `buildry-tofu-state`
- A bucket-scoped R2 token with Object Read & Write permission
- The user kubeconfig available at `~/.kube/config`

The R2 credentials are separate from `CLOUDFLARE_API_TOKEN`. Export their S3
values locally without writing them to the repository:

```bash
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
```

For GitHub Actions, create repository secrets named `R2_ACCESS_KEY_ID` and
`R2_SECRET_ACCESS_KEY`. The existing `CLOUDFLARE_API_TOKEN` secret is also used.
After the state migration succeeds, create a repository variable named
`R2_BACKEND_READY` with the value `true` to enable CI plans and applies.

Migrate the existing local state once, after the bucket and credentials exist:

```bash
make migrate-cloudflare-state
```

OpenTofu stores state at `cloudflare/terraform.tfstate` in the private R2 bucket
and uses an R2 object as a state lock. The ignored local state file is retained
as a migration backup. Pull requests validate and plan infrastructure changes;
changes merged to `main` are applied by GitHub Actions.

Apply the Cloudflare resources and inject the connector token into Kubernetes:

```bash
export CLOUDFLARE_API_TOKEN="..."
make bootstrap-cloudflare
```

After the GitOps change is merged and synchronized, the test application is
available at `https://app.buildry.ca`.

## Portability model

The Git repository is the desired state for Kubernetes and R2 is the state for
Cloudflare account resources. A replacement Linux host only needs k3s, Argo CD,
this repository's root Application, and the cloudflared token Secret. Argo CD
then rebuilds the workloads from Git; no stable residential public IP or inbound
router port is required.
