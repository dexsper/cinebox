#!/usr/bin/env bash
# Refresh crates/cinebox-core/.sqlx after changing query! or migrations.
# Does not edit .cargo/config.toml — SQLX_OFFLINE is overridden for this process only.
#
#   ./scripts/sqlx-prepare.sh
#
# Needs sqlx-cli 0.9.x with sqlite: cargo install sqlx-cli --version 0.9.0 --locked --no-default-features --features sqlite
#
# DATABASE_URL is relative to the workspace root: rustc's cwd is the workspace,
# not the crate, so sqlite:.sqlx-prepare.sqlite would miss the file.

set -euo pipefail

if ! command -v sqlx >/dev/null 2>&1; then
  echo "sqlx-cli is not on PATH. Install: cargo install sqlx-cli --version 0.9.0 --locked --no-default-features --features sqlite" >&2
  exit 1
fi

repo="$(cd "$(dirname "$0")/.." && pwd)"
core="$repo/crates/cinebox-core"
cd "$repo"

export SQLX_OFFLINE=false
export DATABASE_URL=sqlite:crates/cinebox-core/.sqlx-prepare.sqlite

rm -f "$core"/.sqlx-prepare.sqlite "$core"/.sqlx-prepare.sqlite-wal "$core"/.sqlx-prepare.sqlite-shm
sqlx database setup --database-url "$DATABASE_URL" --source "$core/migrations"

rm -rf .sqlx
cargo sqlx prepare --workspace -- --all-targets -p cinebox-core

if [ ! -d .sqlx ]; then
  echo "cargo sqlx prepare did not write .sqlx" >&2
  exit 1
fi

rm -rf "$core/.sqlx"
mv .sqlx "$core/.sqlx"

rm -f "$core"/.sqlx-prepare.sqlite "$core"/.sqlx-prepare.sqlite-wal "$core"/.sqlx-prepare.sqlite-shm
