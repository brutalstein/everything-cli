[CmdletBinding()]
param(
    [switch]$SkipToolchainInstall
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Toolchain = "1.97.1-x86_64-pc-windows-msvc"
$Target = "x86_64-pc-windows-msvc"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TargetDir = Join-Path $RepoRoot "target\verify-windows-msvc"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "scripts/verify-windows.ps1 must run on Windows."
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
    }
}

function Invoke-PinnedCargo {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Arguments
    )

    Invoke-Checked rustup run $Toolchain cargo @Arguments
}

$IsCi = $env:CI -eq "true"
$OverrideNames = @(
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_TARGET",
    "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER",
    "CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER",
    "CC",
    "CXX",
    "AR",
    "CFLAGS",
    "CXXFLAGS",
    "LDFLAGS",
    "CC_x86_64_pc_windows_msvc",
    "CXX_x86_64_pc_windows_msvc",
    "AR_x86_64_pc_windows_msvc",
    "CARGO_TARGET_DIR"
)

$SavedEnvironment = @{}
foreach ($Name in $OverrideNames) {
    $SavedEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
    [Environment]::SetEnvironmentVariable($Name, $null, "Process")
}

try {
    Push-Location $RepoRoot

    if (-not $SkipToolchainInstall) {
        Invoke-Checked rustup toolchain install $Toolchain --profile minimal --component rustfmt --component clippy
    }

    $RustcInfoLines = & rustup run $Toolchain rustc -vV
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to execute rustc from pinned toolchain $Toolchain."
    }
    $RustcInfo = $RustcInfoLines -join "`n"
    if ($RustcInfo -notmatch "(?m)^release:\s+1\.97\.1$") {
        throw "Pinned compiler invariant failed: expected rustc 1.97.1.`n$RustcInfo"
    }
    if ($RustcInfo -notmatch "(?m)^host:\s+x86_64-pc-windows-msvc$") {
        throw "Pinned host invariant failed: expected x86_64-pc-windows-msvc.`n$RustcInfo"
    }

    $env:CARGO_TARGET_DIR = $TargetDir

    if (-not $IsCi) {
        Write-Host "AER Windows verification"
        Write-Host "  toolchain: $Toolchain"
        Write-Host "  target:    $Target"
        Write-Host "  target dir: $TargetDir"
    }

    Invoke-PinnedCargo fmt --all --check
    Invoke-PinnedCargo clippy --locked --workspace --all-targets --target $Target -- -D warnings
    Invoke-PinnedCargo test --locked --workspace --all-targets --target $Target
    Invoke-PinnedCargo test --locked -p aer-storage --all-targets --target $Target
    Invoke-PinnedCargo run --locked --target $Target -p aer-doc-check -- --check
    Invoke-PinnedCargo run --locked --target $Target -p aer-phase0-check -- --check

    Write-Host "AER Windows verification: PASS"
}
finally {
    Pop-Location
    foreach ($Name in $OverrideNames) {
        [Environment]::SetEnvironmentVariable($Name, $SavedEnvironment[$Name], "Process")
    }
}
