# Build an unsigned standalone preview without installing or replacing Handy.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    bun run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
    cargo build --manifest-path src-tauri/Cargo.toml --locked --features tauri/custom-protocol
    if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
    $output = Join-Path $root 'src-tauri\target\voice-preview'
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    Copy-Item 'src-tauri\target\debug\handy.exe' (Join-Path $output 'Handy Voice.exe') -Force
    Copy-Item 'src-tauri\transcribe-libs\*.dll' $output -Force
    Copy-Item 'src-tauri\resources' $output -Recurse -Force
    Write-Output "Standalone preview: $output\Handy Voice.exe"
    Write-Output 'Quit official Handy before using the same keyboard shortcut in this preview.'
} finally { Pop-Location }
