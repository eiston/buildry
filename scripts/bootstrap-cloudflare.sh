#!/usr/bin/env bash
set -euo pipefail

export KUBECONFIG="${KUBECONFIG:-${HOME}/.kube/config}"

for command_name in tofu kubectl; do
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "${command_name} is required." >&2
    exit 1
  fi
done

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  echo "CLOUDFLARE_API_TOKEN is required." >&2
  exit 1
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
terraform_dir="${repo_root}/infrastructure/cloudflare"

tofu -chdir="${terraform_dir}" init
tofu -chdir="${terraform_dir}" apply

token_file="$(mktemp)"
trap 'rm -f "${token_file}"' EXIT
chmod 600 "${token_file}"
tofu -chdir="${terraform_dir}" output -raw tunnel_token >"${token_file}"

kubectl create namespace cloudflare --dry-run=client -o yaml | kubectl apply -f -
kubectl create secret generic cloudflared-token \
  --namespace cloudflare \
  --from-file="token=${token_file}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "Cloudflare resources applied and connector token installed."
