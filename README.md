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

Visit `http://localhost:8081`. The test service remains available locally.

## Cloudflare Tunnel

The Cloudflare account resources are declared under `infrastructure/cloudflare`.
The Kubernetes connector is declared under `clusters/home/platform/cloudflared`.
The connector token is sensitive and is intentionally not committed.

Prerequisites:

- OpenTofu available as `tofu`
- `CLOUDFLARE_API_TOKEN` exported with Tunnel Write and DNS Edit permissions
- The user kubeconfig available at `~/.kube/config`

Apply the Cloudflare resources and inject the connector token into Kubernetes:

```bash
export CLOUDFLARE_API_TOKEN="..."
make bootstrap-cloudflare
```

The public hostname `https://app.buildry.ca` routes to the property catalog
running on the WSL host at `127.0.0.1:8080`. The cloudflared pod uses host
networking so it can reach that loopback address. The app uses PostgreSQL on
`127.0.0.1:5433` and must be built from the separate
`~/git/buildry-property-catalog/apps/property-catalog` worktree.

To keep the app running after the preview terminal closes, build the web app
and API in release mode, then install the user service:

```bash
cd ~/git/buildry-property-catalog/apps/property-catalog
dx build --web --release
cd server
cargo build --release
mkdir -p ~/.config/systemd/user
cp ~/git/buildry/deploy/property-catalog.service ~/.config/systemd/user/
loginctl enable-linger "$USER"
systemctl --user daemon-reload
systemctl --user enable --now property-catalog.service
```

The admin password is set interactively from the server directory with
`./target/release/buildry-property-api set-admin`. The service sets secure
cookies for the public HTTPS hostname. Check it with
`systemctl --user status property-catalog.service`. WSL and the PostgreSQL
container must both remain running for the site to stay online.
