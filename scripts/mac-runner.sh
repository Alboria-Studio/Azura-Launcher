#!/usr/bin/env bash
# Plan B for a macOS job that GitHub leaves without a machine: runs that one job on this Mac, and
# leaves nothing running afterwards.
#
# GitHub's hosted macOS arm64 machines are sometimes short ("capacity constraints"): a job then
# waits in the queue and is cancelled after about 15 minutes without having started. The macOS jobs
# of this repository's Release workflow and of Carolux97/Azura's Runtime kit workflow (which Publish
# kit runs) take their machine from the repository variable MAC_RUNNER, and ask GitHub for
# macos-latest while it is unset. Given the stuck run, this script:
#   1. sets MAC_RUNNER to azura-mac in the run's repository;
#   2. cancels the run if it is still going, re-runs its failed and cancelled jobs, and checks that
#      the macOS job now asks for azura-mac;
#   3. registers this Mac as an ephemeral self-hosted runner with that label: it takes that one job
#      and unregisters itself when the job ends;
#   4. deletes MAC_RUNNER again, also when the script fails or is stopped with Ctrl-C.
#
# Only pushes and manual runs read MAC_RUNNER: pull requests always ask GitHub for a macOS machine,
# so no pull request can run code on this Mac (Azura-Launcher is public). While the script runs,
# another push that starts a macOS job in the same repository would also ask for azura-mac: run it
# when nothing else is being released there.
#
# The job runs as you, with its own home folder under the runner's folder (~/.azura-runner, or
# AZURA_RUNNER_DIR), so git, npm and Rust settings and caches stay out of yours; the Mac is kept
# awake until the job ends. The first time, the script downloads GitHub's runner
# (github.com/actions/runner) into that folder, checked against the SHA-256 in its release notes,
# and again when GitHub releases a new one. Homebrew packages a job installs (the kit's cmake and
# ninja) go into your Homebrew, without updating anything else.
#
# Usage:
#   scripts/mac-runner.sh <run URL>             as GitHub shows it, e.g.
#                                               https://github.com/Carolux97/Azura/actions/runs/123
#   scripts/mac-runner.sh <owner/repo> <run id>
#
# Needs gh signed in with access to the repository's variables, runs and runners.

set -euo pipefail

readonly LABEL="azura-mac"
readonly RUNNER_DIR="${AZURA_RUNNER_DIR:-${HOME}/.azura-runner}"

fail() {
  echo "mac-runner: $*" >&2
  exit 1
}

if [[ $# -eq 1 && "$1" =~ ^https://github\.com/([^/]+/[^/]+)/actions/runs/([0-9]+) ]]; then
  repo="${BASH_REMATCH[1]}"
  run_id="${BASH_REMATCH[2]}"
elif [[ $# -eq 2 && "$1" =~ ^[^/]+/[^/]+$ && "$2" =~ ^[0-9]+$ ]]; then
  repo="$1"
  run_id="$2"
else
  sed -n '/^# Usage:/,/^# Needs/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
fi
readonly repo run_id

[[ "$(uname -sm)" == "Darwin arm64" ]] || fail "this runs the mac-arm64 job, so it needs an Apple Silicon Mac."
gh auth status >/dev/null 2>&1 || fail "gh is not signed in (gh auth login)."

# The runner and the variable are only meant to exist while the job runs: removed on any exit.
cleanup() {
  gh variable delete MAC_RUNNER -R "${repo}" >/dev/null 2>&1 || true
  if [[ -f "${RUNNER_DIR}/runner/.runner" ]]; then
    local remove_token
    if remove_token="$(gh api -X POST "repos/${repo}/actions/runners/remove-token" -q .token 2>/dev/null)"; then
      (cd "${RUNNER_DIR}/runner" && ./config.sh remove --token "${remove_token}" >/dev/null 2>&1) || true
    fi
  fi
  echo "mac-runner: MAC_RUNNER deleted; this Mac is no longer a runner of ${repo}."
}
trap cleanup EXIT

# GitHub's runner, at its latest release: GitHub stops giving jobs to runners that are too old.
latest="$(gh api repos/actions/runner/releases/latest -q .tag_name)"
version="${latest#v}"
if [[ "$(cat "${RUNNER_DIR}/runner/.azura-version" 2>/dev/null)" != "${version}" ]]; then
  echo "mac-runner: downloading GitHub's runner ${version}."
  archive="actions-runner-osx-arm64-${version}.tar.gz"
  expected="$(gh api repos/actions/runner/releases/latest -q .body |
    sed -n 's/.*<!-- BEGIN SHA osx-arm64 -->\([0-9a-f]\{64\}\)<!-- END SHA osx-arm64 -->.*/\1/p')"
  [[ -n "${expected}" ]] || fail "the runner's release notes have no SHA-256 for osx-arm64."
  mkdir -p "${RUNNER_DIR}"
  curl -fsSL --retry 3 -o "${RUNNER_DIR}/${archive}" \
    "https://github.com/actions/runner/releases/download/${latest}/${archive}"
  echo "${expected}  ${RUNNER_DIR}/${archive}" | shasum -a 256 -c - >/dev/null ||
    fail "the downloaded runner does not match its SHA-256."
  rm -rf "${RUNNER_DIR}/runner"
  mkdir -p "${RUNNER_DIR}/runner"
  tar -xzf "${RUNNER_DIR}/${archive}" -C "${RUNNER_DIR}/runner"
  rm "${RUNNER_DIR}/${archive}"
  echo "${version}" > "${RUNNER_DIR}/runner/.azura-version"
fi
mkdir -p "${RUNNER_DIR}/home" "${RUNNER_DIR}/work"

gh variable set MAC_RUNNER -R "${repo}" --body "${LABEL}" >/dev/null
echo "mac-runner: MAC_RUNNER=${LABEL} in ${repo}."

# A run still going is cancelled first: its macOS job may sit queued for GitHub's machine.
if [[ "$(gh run view "${run_id}" -R "${repo}" --json status -q .status)" != "completed" ]]; then
  echo "mac-runner: cancelling run ${run_id}."
  gh run cancel "${run_id}" -R "${repo}" >/dev/null
  while [[ "$(gh run view "${run_id}" -R "${repo}" --json status -q .status)" != "completed" ]]; do
    sleep 5
  done
fi
gh run rerun "${run_id}" -R "${repo}" --failed
echo "mac-runner: re-running the failed and cancelled jobs of run ${run_id}."

# The re-run must ask for this Mac; if its macOS job still asks for GitHub's machine, there is
# nothing for this runner to take.
asks_for_mac=""
for _ in $(seq 1 24); do
  asks_for_mac="$(gh api "repos/${repo}/actions/runs/${run_id}/jobs?filter=latest" \
    -q "[.jobs[] | select(.status != \"completed\") | select(.labels | index(\"${LABEL}\"))] | length")"
  [[ "${asks_for_mac}" -gt 0 ]] && break
  sleep 5
done
[[ "${asks_for_mac}" -gt 0 ]] ||
  fail "no job of the re-run asks for ${LABEL}: it still asks GitHub for a macOS machine."

# Ephemeral: the runner takes one job and unregisters itself. The token is short-lived and only
# passed to the runner's own configuration.
registration_token="$(gh api -X POST "repos/${repo}/actions/runners/registration-token" -q .token)"
(cd "${RUNNER_DIR}/runner" && ./config.sh --unattended --ephemeral --replace --disableupdate \
  --url "https://github.com/${repo}" --token "${registration_token}" \
  --name "${LABEL}-$(hostname -s)" --labels "${LABEL}" --work "${RUNNER_DIR}/work" >/dev/null)
unset registration_token

echo "mac-runner: this Mac is taking the macOS job; it stops by itself when the job ends."
(cd "${RUNNER_DIR}/runner" &&
  HOME="${RUNNER_DIR}/home" \
  HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_UPGRADE=1 HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1 \
  caffeinate -i ./run.sh)

echo "mac-runner: done. The run: https://github.com/${repo}/actions/runs/${run_id}"
