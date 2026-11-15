# Run in PowerShell from the repository root on Windows 11.
# Prerequisites: Node.js 22, Rust MSVC toolchain, Visual Studio C++ Build Tools.
# This checks/builds the app; it does not enable microphones or make AI requests.
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -and $env:OS -ne 'Windows_NT') { throw 'Run this script on Windows.' }
foreach ($program in @('node', 'npm.cmd', 'cargo', 'rustc')) {
    if (-not (Get-Command $program -ErrorAction SilentlyContinue)) { throw "Missing prerequisite: $program" }
}
if (-not ((rustc -vV) -match 'host: .*windows-msvc')) { throw 'Use a Windows MSVC Rust toolchain.' }
& npm.cmd ci
if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
& cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
& npm.cmd run app:build:windows -- --ci
if ($LASTEXITCODE -ne 0) { throw 'Windows build failed.' }
Get-ChildItem 'src-tauri/target/release/bundle/nsis/*-setup.exe' | Select-Object Name, Length, FullName
Write-Host 'Build passed. Install on Windows 11 and run the meeting checks in README before calling the Windows version verified.'
