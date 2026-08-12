param(
    [string]$EnvFile
)

$ErrorActionPreference = "Stop"

# Guard: this script is Windows-only by design (build.rs only calls
# it when cfg!(windows) and no sh is available).
if ($env:OS -ne "Windows_NT") {
    Write-Error "download_lbug.ps1 is a Windows-only script. Use download_lbug.sh on this platform."
    exit 1
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir

# Configuration (env vars with defaults matching the upstream .sh)
$LibKind = if ($env:LBUG_LIB_KIND) { $env:LBUG_LIB_KIND } else { "static" }
$Repository = if ($env:LBUG_GITHUB_REPOSITORY) { $env:LBUG_GITHUB_REPOSITORY } else { "LadybugDB/ladybug" }
$RunId = if ($env:LBUG_PRECOMPILED_RUN_ID) { $env:LBUG_PRECOMPILED_RUN_ID } else { "" }
$VersionOverride = if ($env:LBUG_VERSION) { $env:LBUG_VERSION } else { "" }
$TargetDir = if ($env:LBUG_TARGET_DIR) { $env:LBUG_TARGET_DIR } else { Join-Path $ProjectDir ".cache" "lbug-prebuilt" "lib" }

if ($LibKind -ne "shared" -and $LibKind -ne "static") {
    Write-Error "Unsupported LBUG_LIB_KIND: $LibKind (expected 'shared' or 'static')"
    exit 1
}

$Arch = $env:PROCESSOR_ARCHITECTURE
if ($Arch -eq "AMD64") {
    $Arch = "x86_64"
} elseif ($Arch -eq "ARM64") {
    $Arch = "arm64"
}

# Only x86_64 prebuilt archives are published for Windows.
if ($Arch -ne "x86_64") {
    Write-Error "Unsupported Windows architecture: $Arch (only x86_64 prebuilt archives are published)"
    exit 1
}

if ($LibKind -eq "static") {
    $Archive = "liblbug-static-windows-x86_64.zip"
    $ArtifactName = "liblbug-static-windows-x86_64"
    $LibName = "lbug.lib"
} else {
    $Archive = "liblbug-windows-x86_64.zip"
    $ArtifactName = "liblbug-windows-x86_64"
    $LibName = "lbug_shared.dll"
}

$LibPath = Join-Path $TargetDir $LibName
if (Test-Path $LibPath) {
    Write-Output "liblbug already exists in $TargetDir"
    exit 0
}

New-Item -ItemType Directory -Force $TargetDir | Out-Null
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "lbug-download-$(Get-Random)"
New-Item -ItemType Directory -Force $TmpDir | Out-Null

try {
    $SourceDesc = ""
    if ($RunId) {
        if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
            Write-Error "gh CLI is required when LBUG_PRECOMPILED_RUN_ID is set"
            exit 1
        }
        gh run download $RunId --repo $Repository --name $ArtifactName --dir (Join-Path $TmpDir "artifact") *>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Error "gh run download failed"
            exit 1
        }
        $ExtractedArchive = Get-ChildItem -Path (Join-Path $TmpDir "artifact") -Recurse -File -Name $Archive | Select-Object -First 1
        if (-not $ExtractedArchive) {
            Write-Error "Artifact ${ArtifactName} does not contain ${Archive}"
            exit 1
        }
        Move-Item -Path (Join-Path $TmpDir "artifact" $ExtractedArchive) -Destination (Join-Path $TmpDir $Archive) -Force
        $SourceDesc = "run:${RunId}/${ArtifactName}"
    } else {
        if ($VersionOverride) {
            $Version = $VersionOverride -replace '^v', ''
        } else {
            $Release = Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/${Repository}/releases/latest"
            $Version = $Release.tag_name -replace '^v', ''
        }
        $DownloadUrl = "https://github.com/${Repository}/releases/download/v${Version}/${Archive}"
        Invoke-WebRequest -UseBasicParsing $DownloadUrl -OutFile (Join-Path $TmpDir $Archive)
        $SourceDesc = "release:v${Version}"
    }

    $ArchivePath = Join-Path $TmpDir $Archive
    Expand-Archive -Path $ArchivePath -DestinationPath $TargetDir -Force

    Write-Output "Installed ${Archive} from ${SourceDesc} to $TargetDir"
} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}

if (-not (Test-Path $LibPath)) {
    Write-Error "Expected precompiled library not found at $LibPath"
    exit 1
}

$OutEnvFile = if ($EnvFile) { $EnvFile } else { Join-Path $ProjectDir ".cache" "lbug-prebuilt.env" }
$EnvFileDir = Split-Path -Parent $OutEnvFile
if (-not (Test-Path $EnvFileDir)) {
    New-Item -ItemType Directory -Force $EnvFileDir | Out-Null
}
@"
LBUG_LIBRARY_DIR=$TargetDir
LBUG_INCLUDE_DIR=$TargetDir
"@ | Set-Content -Path $OutEnvFile

Write-Output "Wrote $OutEnvFile"
Write-Output "Resolved precompiled library: $LibPath"
