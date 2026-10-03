#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="${PROPERTY_CATALOG_DIR:-$(dirname -- "${repo_root}")/buildry-property-catalog/apps/property-catalog}"

if [[ ! -f "${app_dir}/Cargo.toml" || ! -f "${app_dir}/server/Cargo.toml" ]]; then
  echo "Property catalog source not found at ${app_dir}." >&2
  exit 1
fi

export PATH="${HOME}/.cargo/bin:${HOME}/.dx/bin:${PATH}"
for command_name in cc cargo dx curl systemctl; do
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "${command_name} is required." >&2
    exit 1
  fi
done

export CC=/usr/bin/cc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/cc

cd -- "${app_dir}"
dx build --web --release
(cd server && cargo build --release)
systemctl --user restart property-catalog.service
systemctl --user is-active --quiet property-catalog.service
curl --fail --silent --show-error --retry 3 --retry-delay 1 --output /dev/null https://app.buildry.ca/login
curl --fail --silent --show-error --retry 3 --retry-delay 1 --output /dev/null https://app.buildry.ca/book

echo "Published https://app.buildry.ca"
