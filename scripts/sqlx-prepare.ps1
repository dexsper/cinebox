# Refresh crates/cinebox-core/.sqlx after changing query! or migrations.
# Does not edit .cargo/config.toml — SQLX_OFFLINE is overridden for this process only.
#
#   pwsh scripts/sqlx-prepare.ps1
#
# Needs sqlx-cli 0.9.x with sqlite: cargo install sqlx-cli --version 0.9.0 --locked --no-default-features --features sqlite
#
# DATABASE_URL is relative to the workspace root: rustc's cwd is the workspace,
# not the crate, so sqlite:.sqlx-prepare.sqlite would miss the file.

$ErrorActionPreference = "Stop"

if (-not (Get-Command sqlx -ErrorAction SilentlyContinue)) {
    Write-Error "sqlx-cli is not on PATH. Install: cargo install sqlx-cli --version 0.9.0 --locked --no-default-features --features sqlite"
}

$repo = Split-Path -Parent $PSScriptRoot
$core = Join-Path $repo "crates\cinebox-core"
Set-Location $repo

$env:SQLX_OFFLINE = "false"
$env:DATABASE_URL = "sqlite:crates/cinebox-core/.sqlx-prepare.sqlite"

Get-ChildItem -Path $core -Filter ".sqlx-prepare.sqlite*" -ErrorAction SilentlyContinue |
    Remove-Item -Force

sqlx database setup --database-url $env:DATABASE_URL --source (Join-Path $core "migrations")

if (Test-Path .sqlx) {
    Remove-Item -Recurse -Force .sqlx
}

cargo sqlx prepare --workspace -- --all-targets -p cinebox-core

if (-not (Test-Path .sqlx)) {
    Write-Error "cargo sqlx prepare did not write .sqlx"
}

$crateSqlx = Join-Path $core ".sqlx"
if (Test-Path $crateSqlx) {
    Remove-Item -Recurse -Force $crateSqlx
}

Move-Item .sqlx $crateSqlx

Get-ChildItem -Path $core -Filter ".sqlx-prepare.sqlite*" -ErrorAction SilentlyContinue |
    Remove-Item -Force
