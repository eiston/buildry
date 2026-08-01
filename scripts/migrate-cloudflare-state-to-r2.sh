#!/usr/bin/env bash
set -euo pipefail

for command_name in tofu; do
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "${command_name} is required." >&2
    exit 1
  fi
done

for variable_name in AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY; do
  if [[ -z "${!variable_name:-}" ]]; then
    echo "${variable_name} is required for the R2 state backend." >&2
    exit 1
  fi
done

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
terraform_dir="${repo_root}/infrastructure/cloudflare"
local_state="${terraform_dir}/terraform.tfstate"

if [[ ! -s "${local_state}" ]]; then
  echo "No local state found at ${local_state}; nothing to migrate." >&2
  exit 1
fi

echo "Migrating the existing Cloudflare state to the private R2 backend."
echo "The local state file will be retained as a backup and remains git-ignored."
cp -p "${local_state}" "${local_state}.pre-r2"
tofu -chdir="${terraform_dir}" init -migrate-state

echo "R2 state migration completed."
