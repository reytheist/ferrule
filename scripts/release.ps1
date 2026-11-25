#!/usr/bin/env pwsh
param(
    [switch]$SkipVerify
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][scriptblock]$Command
    )
    Write-Host "==> $Label"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed ($LASTEXITCODE): $Label"
    }
}

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $RepoRoot
try {
    $Pkg = Get-Content (Join-Path $RepoRoot "package.json") -Raw | ConvertFrom-Json
    $Version = $Pkg.version
    if ([string]::IsNullOrWhiteSpace($Version)) {
        throw "Could not determine version from package.json"
    }

    if (-not $SkipVerify) {
        Invoke-Checked "Local verification (pnpm verify)" { pnpm verify }
    } else {
        Write-Host "==> Skipping pnpm verify (-SkipVerify)"
    }

    Invoke-Checked "Build staff-sync-server release binary (pnpm build:server)" { pnpm build:server }
    Invoke-Checked "Build Ferrule desktop installers (pnpm tauri build)" { pnpm tauri build }

    $OutDir = Join-Path $RepoRoot "release-artifacts\v$Version"
    if (Test-Path $OutDir) {
        Remove-Item $OutDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

    $ArchRaw = if ($env:PROCESSOR_ARCHITECTURE) { $env:PROCESSOR_ARCHITECTURE.ToLowerInvariant() } else { "x64" }
    $Arch = switch ($ArchRaw) {
        "amd64" { "x64" }
        "x86_64" { "x64" }
        "arm64" { "arm64" }
        default { $ArchRaw }
    }

    $Staged = [System.Collections.Generic.List[string]]::new()

    $BundleRoot = Join-Path $RepoRoot "target\release\bundle"
    $Patterns = @(
        (Join-Path $BundleRoot "msi\*.msi"),
        (Join-Path $BundleRoot "nsis\*-setup.exe")
    )
    foreach ($Pattern in $Patterns) {
        foreach ($File in (Get-ChildItem -Path $Pattern -ErrorAction SilentlyContinue)) {
            $Dest = Join-Path $OutDir $File.Name
            Copy-Item -Path $File.FullName -Destination $Dest -Force
            $Staged.Add($File.Name)
        }
    }

    $ServerBin = Join-Path $RepoRoot "target\release\staff-sync-server.exe"
    if (Test-Path $ServerBin) {
        $ServerDestName = "staff-sync-server-v$Version-windows-$Arch.exe"
        Copy-Item -Path $ServerBin -Destination (Join-Path $OutDir $ServerDestName) -Force
        $Staged.Add($ServerDestName)
    } else {
        throw "Expected staff-sync-server binary not found at $ServerBin"
    }

    if ($Staged.Count -eq 0) {
        throw "No release artifacts were found to stage."
    }

    $Lines = [System.Collections.Generic.List[string]]::new()
    foreach ($Name in ($Staged | Sort-Object)) {
        $FullPath = Join-Path $OutDir $Name
        $Hash = (Get-FileHash -Path $FullPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $Lines.Add("$Hash  $Name")
    }

    $ChecksumPath = Join-Path $OutDir "SHA256SUMS.txt"
    [System.IO.File]::WriteAllText($ChecksumPath, (($Lines -join "`n") + "`n"), [System.Text.UTF8Encoding]::new($false))

    Write-Host ""
    Write-Host "==> Staged Ferrule v$Version release artifacts in: $OutDir"
    foreach ($Line in $Lines) {
        Write-Host "    $Line"
    }
} finally {
    Pop-Location
}
