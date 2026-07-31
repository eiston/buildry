#!/usr/bin/env bash
set -euo pipefail

K3S_VERSION="${K3S_VERSION:-v1.36.2+k3s1}"
ARGOCD_VERSION="${ARGOCD_VERSION:-v3.4.5}"
TARGET_USER="${TARGET_USER:-${SUDO_USER:-}}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run with sudo: sudo TARGET_USER=\$USER ./scripts/bootstrap.sh" >&2
  exit 1
fi

if [[ -z "${TARGET_USER}" || "${TARGET_USER}" == "root" ]]; then
  echo "TARGET_USER must name the non-root WSL user." >&2
  exit 1
fi

if ! systemctl is-system-running >/dev/null 2>&1; then
  echo "systemd is not running. Enable it in /etc/wsl.conf first." >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  apt-get update
  apt-get install -y curl
fi

if [[ ! -f /etc/systemd/system/k3s.service ]]; then
  curl --fail --silent --show-error --location https://get.k3s.io |
    INSTALL_K3S_VERSION="${K3S_VERSION}" \
    INSTALL_K3S_EXEC="server --write-kubeconfig-mode=600" sh -
else
  systemctl enable --now k3s
fi

for _ in $(seq 1 60); do
  if k3s kubectl get nodes >/dev/null 2>&1; then
    break
  fi
  sleep 2
done

k3s kubectl wait --for=condition=Ready node --all --timeout=180s

target_home="$(getent passwd "${TARGET_USER}" | cut -d: -f6)"
install -d -m 700 -o "${TARGET_USER}" -g "${TARGET_USER}" "${target_home}/.kube"
install -m 600 -o "${TARGET_USER}" -g "${TARGET_USER}" \
  /etc/rancher/k3s/k3s.yaml "${target_home}/.kube/config"

if ! k3s kubectl get namespace argocd >/dev/null 2>&1; then
  k3s kubectl create namespace argocd
fi

k3s kubectl apply --server-side --force-conflicts -n argocd \
  -f "https://raw.githubusercontent.com/argoproj/argo-cd/${ARGOCD_VERSION}/manifests/install.yaml"

k3s kubectl rollout status deployment/argocd-server -n argocd --timeout=300s
k3s kubectl rollout status statefulset/argocd-application-controller \
  -n argocd --timeout=300s

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
k3s kubectl apply -f "${script_dir}/../bootstrap/root-application.yaml"

echo
echo "Bootstrap complete."
echo "Run as ${TARGET_USER}: cd ~/git/buildry && make status"
