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
$EverythingExe = Join-Path $TargetDir "$Target\debug\everything.exe"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "scripts/verify-windows.ps1 must run on Windows."
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
    }
}

function Resolve-RustupTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Tool
    )

    $Output = & rustup which --toolchain $Toolchain $Tool
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to resolve $Tool from pinned toolchain $Toolchain."
    }
    $Path = ($Output | Select-Object -First 1).Trim()
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "rustup returned an invalid $Tool path: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Remove-ProcessEnvironmentVariable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $Path = "Env:$Name"
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Force
    }
}

function Restore-ProcessEnvironmentVariable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [AllowNull()]
        [string]$Value
    )

    if ($null -eq $Value) {
        Remove-ProcessEnvironmentVariable -Name $Name
    }
    else {
        Set-Item -LiteralPath "Env:$Name" -Value $Value
    }
}

$IsCi = $env:CI -eq "true"
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
    "CLIPPY_ARGS",
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
    Remove-ProcessEnvironmentVariable -Name $Name
}
$OriginalPath = $env:PATH

$LocationPushed = $false
try {
    Push-Location $RepoRoot
    $LocationPushed = $true

    if (-not $SkipToolchainInstall) {
        Invoke-Checked -FilePath "rustup" -Arguments @(
            "toolchain", "install", $Toolchain,
            "--profile", "minimal",
            "--component", "rustfmt",
            "--component", "clippy"
        )
    }

    $CargoPath = Resolve-RustupTool -Tool "cargo"
    $RustcPath = Resolve-RustupTool -Tool "rustc"
    $RustdocPath = Resolve-RustupTool -Tool "rustdoc"
    $ToolchainBin = Split-Path -Parent $CargoPath

    $env:PATH = "$ToolchainBin;$OriginalPath"
    $env:CARGO = $CargoPath
    $env:RUSTC = $RustcPath
    $env:RUSTDOC = $RustdocPath
    $env:CARGO_TARGET_DIR = $TargetDir

    $RustcInfoLines = & $RustcPath -vV
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to execute pinned rustc at $RustcPath."
    }
    $RustcInfo = $RustcInfoLines -join "`n"
    if ($RustcInfo -notmatch "(?m)^release:\s+1\.97\.1$") {
        throw "Pinned compiler invariant failed: expected rustc 1.97.1.`n$RustcInfo"
    }
    if ($RustcInfo -notmatch "(?m)^host:\s+x86_64-pc-windows-msvc$") {
        throw "Pinned host invariant failed: expected x86_64-pc-windows-msvc.`n$RustcInfo"
    }

    if (-not $IsCi) {
        Write-Host "everything Windows verification"
        Write-Host "  toolchain:  $Toolchain"
        Write-Host "  cargo:      $CargoPath"
        Write-Host "  rustc:      $RustcPath"
        Write-Host "  target:     $Target"
        Write-Host "  target dir: $TargetDir"
    }

    Invoke-Checked -FilePath $CargoPath -Arguments @("fmt", "--all", "--check")
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "clippy", "--locked", "--workspace", "--all-targets", "--target", $Target,
        "--", "-D", "warnings"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "--workspace", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "--test", "provider_router_bench", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "-p", "aer-core", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-domain", "-p", "aer-research", "-p", "aer-core", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-repo", "-p", "aer-core", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-repo", "--test", "repo_intel_2_bench", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-repo", "--test", "repo_intel_2_tier2", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--test", "handoff_bench", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--test", "resource_bench", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-context", "-p", "aer-repo", "-p", "aer-core", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--all-targets", "--target", $Target, "verification"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-storage", "--all-targets", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "run", "--locked", "--target", $Target, "-p", "aer-doc-check", "--", "--check"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "run", "--locked", "--target", $Target, "-p", "aer-phase0-check", "--", "--check"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "build", "--locked", "--target", $Target, "-p", "everything"
    )

    if (-not (Test-Path -LiteralPath $EverythingExe -PathType Leaf)) {
        throw "everything binary was not produced at expected path: $EverythingExe"
    }

    Write-Host "everything Windows verification: PASS"
    if (-not $IsCi) {
        Write-Host "  product:    $EverythingExe"
        Write-Host "  launch:     & `"$EverythingExe`""
    }
}
finally {
    if ($LocationPushed) {
        Pop-Location
    }
    $env:PATH = $OriginalPath
    foreach ($Name in $OverrideNames) {
        Restore-ProcessEnvironmentVariable -Name $Name -Value $SavedEnvironment[$Name]
    }
}
