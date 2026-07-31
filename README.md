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
