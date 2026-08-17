[CmdletBinding()]
param(
    [ValidateRange(1, 3)]
    [int]$Runs = 2,
    [string]$Task,
    [string]$Model,
    [switch]$Live,
    [switch]$Json,
    [switch]$VerifyRepo
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Toolchain = "1.97.1-x86_64-pc-windows-msvc"
$Target = "x86_64-pc-windows-msvc"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TargetDir = Join-Path $RepoRoot "target\verify-windows-msvc"
$Binary = Join-Path $TargetDir "$Target\debug\aer-provider-acceptance.exe"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "scripts/run-provider-acceptance-windows.ps1 must run on Windows."
}

if ($VerifyRepo) {
    & (Join-Path $PSScriptRoot "verify-windows.ps1") -SkipToolchainInstall
    if ($LASTEXITCODE -ne 0) {
        throw "Canonical Windows verification failed before provider acceptance."
    }
}

function Resolve-RustupTool {
    param([Parameter(Mandatory = $true)][string]$Tool)

    $Output = & rustup which --toolchain $Toolchain $Tool
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to resolve $Tool from $Toolchain. Install it with: rustup toolchain install $Toolchain --profile minimal"
    }
    $Path = ($Output | Select-Object -First 1).Trim()
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "rustup returned an invalid $Tool path: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

$OverrideNames = @(
    "RUSTC",
    "RUSTDOC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTC",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_TARGET",
    "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER",
    "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS",
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

$Saved = @{}
foreach ($Name in $OverrideNames) {
    $Saved[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
    Remove-Item -LiteralPath "Env:$Name" -Force -ErrorAction SilentlyContinue
}
$OriginalPath = $env:PATH
$LocationPushed = $false

try {
    Push-Location $RepoRoot
    $LocationPushed = $true

    $CargoPath = Resolve-RustupTool -Tool "cargo"
    $RustcPath = Resolve-RustupTool -Tool "rustc"
    $ToolchainBin = Split-Path -Parent $CargoPath
    $env:PATH = "$ToolchainBin;$OriginalPath"
    $env:CARGO = $CargoPath
    $env:RUSTC = $RustcPath
    $env:CARGO_TARGET_DIR = $TargetDir

    $RustcInfo = (& $RustcPath -vV) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $RustcInfo -notmatch "(?m)^release:\s+1\.97\.1$") {
        throw "Pinned compiler invariant failed: expected rustc 1.97.1.`n$RustcInfo"
    }
    if ($RustcInfo -notmatch "(?m)^host:\s+x86_64-pc-windows-msvc$") {
        throw "Pinned host invariant failed: expected x86_64-pc-windows-msvc.`n$RustcInfo"
    }

    & $CargoPath build --quiet --locked --target $Target -p everything --bin aer-provider-acceptance
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build aer-provider-acceptance with canonical MSVC toolchain."
    }
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
        throw "Acceptance binary was not produced at expected path: $Binary"
    }

    $Arguments = @("--workspace", $RepoRoot, "--runs", "$Runs")
    if ($Task) {
        $Arguments += @("--task", $Task)
    }
    if ($Model) {
        $Arguments += @("--model", $Model)
    }
    if ($Live) {
        $Arguments += "--live"
    }
    if ($Json) {
        $Arguments += "--json"
    }

    & $Binary @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "aer-provider-acceptance exited with code $LASTEXITCODE"
    }
}
finally {
    if ($LocationPushed) {
        Pop-Location
    }
    $env:PATH = $OriginalPath
    foreach ($Name in $OverrideNames) {
        if ($null -eq $Saved[$Name]) {
            Remove-Item -LiteralPath "Env:$Name" -Force -ErrorAction SilentlyContinue
        }
        else {
            Set-Item -LiteralPath "Env:$Name" -Value $Saved[$Name]
        }
    }
}
