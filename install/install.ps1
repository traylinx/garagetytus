# garagetytus web bootstrap — Windows.
#
# v0.1: Garage upstream ships no Windows binary, and the v0.1 budget
# can't carry our own Windows build pipeline. This bootstrapper
# prints the deferral notice and exits 0 (planned deferral, not an
# error).
#
# Windows reopens at v0.2 with explicit scope:
#   * build-our-own Garage Windows binary via CI, OR
#   * official WSL2 path, OR
#   * drop the Windows lane permanently.
#
# Decision recorded in:
#   development/sprints/queued/GARAGETYTUS-V0.1/verdicts/Q1-VERDICT.md
#   (lope, pi+codex 2026-04-25, both PASS Option A.)

Write-Host "garagetytus v0.1 ships macOS + Linux only."
Write-Host "Windows support targets v0.2."
Write-Host ""
Write-Host "Track v0.2 progress at:"
Write-Host "  https://github.com/traylinx/garagetytus/issues?q=label%3Av0.2"
Write-Host ""
Write-Host "If you need a local S3 daemon on Windows today, options:"
Write-Host "  - WSL2 + the Linux install path:"
Write-Host "      wsl curl -fsSL https://garagetytus.dev/install | sh"
Write-Host "  - Docker Desktop + the Linux container."
Write-Host ""
Write-Host "Both are unsupported in v0.1; v0.2 will pick a first-class path."

# Exit 0 — planned deferral, not an error.
exit 0
