param(
    [string]$EnvFile
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir

$LibKind = if ($env:LBUG_LIB_KIND) { $env:LBUG_LIB_KIND } else { "static" }
$LinuxVariant = if ($env:LBUG_LINUX_VARIANT) { $env:LBUG_LINUX_VARIANT } else { "compat" }
$Repository = if ($env:LBUG_GITHUB_REPOSITORY) { $env:LBUG_GITHUB_REPOSITORY } else { "LadybugDB/ladybug" }
$RunId = if ($env:LBUG_PRECOMPILED_RUN_ID) { $env:LBUG_PRECOMPILED_RUN_ID } else { "" }
$VersionOverride = if ($env:LBUG_VERSION) { $env:LBUG_VERSION } else { "" }

if ($LibKind -ne "shared" -and $LibKind -ne "static") {
    Write-Error "Unsupported LBUG_LIB_KIND: $LibKind (expected 'shared' or 'static')"
    exit 1
}

if ($LinuxVariant -ne "compat" -and $LinuxVariant -ne "perf") {
    Write-Error "Unsupported LBUG_LINUX_VARIANT: $LinuxVariant (expected 'compat' or 'perf')"
    exit 1
}

$TargetDir = if ($env:LBUG_TARGET_DIR) { $env:LBUG_TARGET_DIR } else { Join-Path $ProjectDir ".cache" "lbug-prebuilt" "lib" }

$Os = ""
$Arch = ""

if ($IsWindows) {
    $Os = "windows"
    $Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "AMD64") { "x86_64" } elseif ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { $env:PROCESSOR_ARCHITECTURE }
} elseif ($IsMacOS) {
    $Os = "macos"
    $Arch = if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) { "arm64" } else { "x86_64" }
} elseif ($IsLinux) {
    $Os = "linux"
    $Arch = if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) { "aarch64" } else { "x86_64" }
} else {
    $Os = "windows"
    $Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "AMD64") { "x86_64" } elseif ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { $env:PROCESSOR_ARCHITECTURE }
}

$Archive = ""
$LibName = ""
$ArtifactName = ""

switch ($Os) {
    "macos" {
        if ($Arch -ne "x86_64" -and $Arch -ne "arm64") {
            Write-Error "Unsupported macOS architecture: $Arch"
            exit 1
        }
        if ($LibKind -eq "static") {
            $Archive = "liblbug-static-osx-${Arch}.tar.gz"
            $ArtifactName = "liblbug-static-osx-${Arch}"
            $LibName = "liblbug.a"
        } else {
            $Archive = "liblbug-osx-${Arch}.tar.gz"
            $ArtifactName = "liblbug-osx-${Arch}"
            $LibName = "liblbug.dylib"
        }
    }
    "linux" {
        if ($Arch -ne "x86_64" -and $Arch -ne "aarch64") {
            Write-Error "Unsupported Linux architecture: $Arch"
            exit 1
        }
        if ($LibKind -eq "static") {
            $Archive = "liblbug-static-linux-${Arch}-${LinuxVariant}.tar.gz"
            $ArtifactName = "liblbug-static-linux-${Arch}-${LinuxVariant}"
            $LibName = "liblbug.a"
        } else {
            $Archive = "liblbug-linux-${Arch}.tar.gz"
            $ArtifactName = "liblbug-linux-${Arch}"
            $LibName = "liblbug.so"
        }
    }
    "windows" {
        if ($Arch -ne "x86_64") {
            Write-Error "Unsupported Windows architecture: $Arch"
            exit 1
        }
        if ($LibKind -eq "static") {
            $Archive = "liblbug-static-windows-${Arch}.zip"
            $ArtifactName = "liblbug-static-windows-${Arch}"
            $LibName = "lbug.lib"
        } else {
            $Archive = "liblbug-windows-${Arch}.zip"
            $ArtifactName = "liblbug-windows-${Arch}"
            $LibName = "lbug_shared.dll"
        }
    }
    default {
        Write-Error "Unsupported OS: $Os"
        exit 1
    }
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
    if ($Archive.EndsWith(".zip")) {
        Expand-Archive -Path $ArchivePath -DestinationPath $TargetDir -Force
    } else {
        tar xzf $ArchivePath -C $TargetDir
    }

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
