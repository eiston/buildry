#!/usr/bin/env bash
set -euo pipefail

export KUBECONFIG="${KUBECONFIG:-${HOME}/.kube/config}"

if ! command -v kubectl >/dev/null 2>&1; then
  echo "kubectl is unavailable; run make bootstrap first." >&2
  exit 1
fi

echo "Nodes"
kubectl get nodes -o wide
echo
echo "Argo CD"
kubectl get pods -n argocd
echo
echo "GitOps applications"
kubectl get applications -n argocd
