param(
    [switch]$SkipBuild
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = (Resolve-Path (Join-Path $scriptDir "..")).Path
$versionFile = Join-Path $root "third_party\mupdf.version"

if (!(Test-Path -LiteralPath $versionFile)) {
    throw "missing MuPDF version file: $versionFile"
}

function Get-EnvOrDefault {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Default
    )

    $item = Get-Item -Path "Env:$Name" -ErrorAction SilentlyContinue
    if ($null -eq $item -or $null -eq $item.Value -or $item.Value.Trim().Length -eq 0) {
        return $Default
    }
    return $item.Value
}

function Convert-ToAbsolutePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = Resolve-Path -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($null -ne $resolved) {
        return $resolved.Path
    }
    if (Split-Path -IsAbsolute $Path) {
        return $Path
    }
    return Join-Path (Get-Location).Path $Path
}

function Read-KeyValueFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    $values = @{}
    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = $line.Trim()
        if ($trimmed.Length -eq 0 -or $trimmed.StartsWith("#")) {
            continue
        }
        $parts = $trimmed.Split("=", 2)
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Write-GeneratedFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    Set-Content -LiteralPath $Path -Value $Content -Encoding utf8
}

function Quote-PowerShellValue {
    param([Parameter(Mandatory = $true)][string]$Value)

    return "'" + ($Value -replace "'", "''") + "'"
}

function Escape-XmlText {
    param([Parameter(Mandatory = $true)][string]$Value)

    return ($Value -replace "&", "&amp;" -replace "<", "&lt;" -replace ">", "&gt;" -replace '"', "&quot;" -replace "'", "&apos;")
}

function Find-VcVars64 {
    $explicit = Get-EnvOrDefault "SCALPEL_MUPDF_VCVARS64" ""
    if ($explicit.Trim().Length -ne 0) {
        $explicitPath = Convert-ToAbsolutePath $explicit
        if (Test-Path -LiteralPath $explicitPath) {
            return $explicitPath
        }
        throw "SCALPEL_MUPDF_VCVARS64 does not exist: $explicitPath"
    }

    $vswhere = Get-EnvOrDefault "SCALPEL_MUPDF_VSWHERE" ""
    if ($vswhere.Trim().Length -eq 0) {
        $programFilesX86 = ${env:ProgramFiles(x86)}
        $vswhere = Join-Path $programFilesX86 "Microsoft Visual Studio\Installer\vswhere.exe"
    }
    if (!(Test-Path -LiteralPath $vswhere)) {
        throw "vswhere.exe was not found. Set SCALPEL_MUPDF_VCVARS64 or SCALPEL_MUPDF_VSWHERE."
    }

    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($null -eq $vsPath -or $vsPath.Trim().Length -eq 0) {
        throw "Visual Studio with MSVC x64 tools was not found"
    }

    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
    if (!(Test-Path -LiteralPath $vcvars)) {
        throw "vcvars64.bat was not found: $vcvars"
    }
    return $vcvars
}

$metadata = Read-KeyValueFile $versionFile
$mupdfVersion = $metadata["MUPDF_VERSION"]
$sourceUrl = $metadata["MUPDF_SOURCE_URL"]
$expectedSha = $metadata["MUPDF_SHA256"]
if ($null -eq $mupdfVersion -or $mupdfVersion.Trim().Length -eq 0) {
    throw "missing MUPDF_VERSION in $versionFile"
}
if ($null -eq $sourceUrl -or $sourceUrl.Trim().Length -eq 0) {
    throw "missing MUPDF_SOURCE_URL in $versionFile"
}

$thirdPartyDir = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_THIRD_PARTY_DIR" (Join-Path $root "third_party"))
$cacheDir = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_CACHE_DIR" (Join-Path $thirdPartyDir "cache"))
$archive = Join-Path $cacheDir "mupdf-$mupdfVersion-source.tar.gz"
$archivePart = "$archive.part"
$sourceDir = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_SOURCE_DIR" (Join-Path $thirdPartyDir "mupdf-$mupdfVersion-source"))
$envFile = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_ENV_FILE" (Join-Path $thirdPartyDir "mupdf.env.ps1"))
$cmdEnvFile = $envFile -replace "\.ps1$", ".cmd"
if ($cmdEnvFile -eq $envFile) {
    $cmdEnvFile = "$envFile.cmd"
}
$platform = Get-EnvOrDefault "SCALPEL_MUPDF_PLATFORM" "x64"
$configuration = Get-EnvOrDefault "SCALPEL_MUPDF_CONFIGURATION" "Release"
$toolset = Get-EnvOrDefault "SCALPEL_MUPDF_PLATFORM_TOOLSET" "v143"
$localAppData = Get-EnvOrDefault "LOCALAPPDATA" $thirdPartyDir
$msvcBuildDir = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_MSVC_BUILD_DIR" (Join-Path $localAppData "Temp\Scalpel\mupdf-$mupdfVersion-msvc"))

New-Item -ItemType Directory -Force -Path $cacheDir | Out-Null

$archiveMatchesChecksum = $false
if (Test-Path -LiteralPath $archive) {
    if ($null -eq $expectedSha -or $expectedSha.Trim().Length -eq 0) {
        $archiveMatchesChecksum = $true
    } else {
        $actualSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
        $archiveMatchesChecksum = $actualSha -eq $expectedSha.ToLowerInvariant()
    }
}

if ($archiveMatchesChecksum) {
    Write-Host "using cached archive $archive"
} else {
    Write-Host "downloading MuPDF $mupdfVersion"
    if (Test-Path -LiteralPath $archivePart) {
        Remove-Item -LiteralPath $archivePart -Force
    }
    Invoke-WebRequest -Uri $sourceUrl -OutFile $archivePart
    Move-Item -LiteralPath $archivePart -Destination $archive -Force
}

if ($null -ne $expectedSha -and $expectedSha.Trim().Length -ne 0) {
    $actualSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
    if ($actualSha -ne $expectedSha.ToLowerInvariant()) {
        throw "MuPDF archive checksum mismatch. expected=$expectedSha actual=$actualSha"
    }
}

if (!(Test-Path -LiteralPath $sourceDir)) {
    $tmpDir = Join-Path $thirdPartyDir ".mupdf-extract-$PID"
    if (Test-Path -LiteralPath $tmpDir) {
        Remove-Item -LiteralPath $tmpDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $sourceDir) | Out-Null

    Write-Host "extracting MuPDF to $sourceDir"
    tar -xzf $archive -C $tmpDir
    if ($LASTEXITCODE -ne 0) {
        throw "failed to extract $archive"
    }
    $extractedDir = Get-ChildItem -LiteralPath $tmpDir -Directory | Select-Object -First 1
    if ($null -eq $extractedDir) {
        throw "MuPDF archive did not contain a source directory"
    }
    Move-Item -LiteralPath $extractedDir.FullName -Destination $sourceDir
    Remove-Item -LiteralPath $tmpDir -Force
} else {
    Write-Host "using existing source tree $sourceDir"
}

$requiredHeaders = @(
    (Join-Path $sourceDir "include\mupdf\fitz.h"),
    (Join-Path $sourceDir "include\mupdf\pdf.h"),
    (Join-Path $sourceDir "include\mupdf\pdf\javascript.h")
)
foreach ($header in $requiredHeaders) {
    if (!(Test-Path -LiteralPath $header)) {
        throw "MuPDF source tree is missing required header: $header"
    }
}

$win32Dir = Join-Path $sourceDir "platform\win32"
$outRoot = Join-Path $msvcBuildDir "out"
$intRoot = Join-Path $msvcBuildDir "int"
$libDir = Convert-ToAbsolutePath (Get-EnvOrDefault "SCALPEL_MUPDF_LIB_DIR" (Join-Path (Join-Path $outRoot $platform) $configuration))
$mutool = Join-Path $libDir "mutool.exe"

if ($SkipBuild) {
    $env:SCALPEL_MUPDF_SKIP_BUILD = "1"
}

if ($env:SCALPEL_MUPDF_SKIP_BUILD -ne "1") {
    if (!(Test-Path -LiteralPath $win32Dir)) {
        throw "MuPDF Windows project directory was not found: $win32Dir"
    }

    $targetsFile = Join-Path $win32Dir "Directory.Build.targets"
    if (Test-Path -LiteralPath $targetsFile) {
        $existingTargets = Get-Content -LiteralPath $targetsFile -Raw
        if ($existingTargets -notmatch "Generated by Scalpel setup-mupdf.ps1") {
            throw "refusing to overwrite existing MuPDF Directory.Build.targets: $targetsFile"
        }
    }

    New-Item -ItemType Directory -Force -Path $outRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $intRoot | Out-Null

    $escapedOutRoot = Escape-XmlText $outRoot
    $escapedIntRoot = Escape-XmlText $intRoot
    $targetsContent = @"
<Project>
  <!-- Generated by Scalpel setup-mupdf.ps1. Keeps local MSVC outputs away from upstream defaults. -->
  <PropertyGroup>
    <OutDir>$escapedOutRoot\`$(Platform)\`$(Configuration)\</OutDir>
    <IntDir>$escapedIntRoot\`$(Platform)\`$(Configuration)\`$(MSBuildProjectName)\</IntDir>
    <TargetDir>`$(OutDir)</TargetDir>
    <TargetPath>`$(OutDir)`$(TargetName)`$(TargetExt)</TargetPath>
  </PropertyGroup>
</Project>
"@
    Write-GeneratedFile $targetsFile $targetsContent

    $vcvars = Find-VcVars64
    Push-Location $win32Dir
    try {
        Write-Host "building MuPDF $mupdfVersion with $toolset ($platform $configuration)"
        $cmdLine = "`"$vcvars`" && MSBuild.exe `"mupdf.sln`" /m:1 /nr:false /t:libmupdf /p:Configuration=$configuration /p:Platform=$platform /p:PlatformToolset=$toolset /v:minimal"
        & cmd.exe /s /c $cmdLine
        if ($LASTEXITCODE -ne 0) {
            throw "MuPDF Windows build failed"
        }
    } finally {
        Pop-Location
    }
}

$libmupdf = Join-Path $libDir "libmupdf.lib"
if (!(Test-Path -LiteralPath $libmupdf)) {
    throw "MuPDF libmupdf.lib was not found at $libmupdf"
}
if ((Get-Item -LiteralPath $libmupdf).Length -le 0) {
    throw "MuPDF libmupdf.lib is empty: $libmupdf"
}

$envDir = Split-Path -Parent $envFile
New-Item -ItemType Directory -Force -Path $envDir | Out-Null

$psEnvLines = @(
    "`$env:SCALPEL_MUPDF_SOURCE_DIR = $(Quote-PowerShellValue $sourceDir)",
    "`$env:SCALPEL_MUPDF_INCLUDE_DIR = $(Quote-PowerShellValue (Join-Path $sourceDir "include"))",
    "`$env:SCALPEL_MUPDF_LIB_DIR = $(Quote-PowerShellValue $libDir)"
)
$cmdEnvLines = @(
    "@echo off",
    "set `"SCALPEL_MUPDF_SOURCE_DIR=$sourceDir`"",
    "set `"SCALPEL_MUPDF_INCLUDE_DIR=$(Join-Path $sourceDir "include")`"",
    "set `"SCALPEL_MUPDF_LIB_DIR=$libDir`""
)
if (Test-Path -LiteralPath $mutool) {
    $psEnvLines += "`$env:SCALPEL_MUTOOL_PATH = $(Quote-PowerShellValue $mutool)"
    $cmdEnvLines += "set `"SCALPEL_MUTOOL_PATH=$mutool`""
}

$env:SCALPEL_MUPDF_SOURCE_DIR = $sourceDir
$env:SCALPEL_MUPDF_INCLUDE_DIR = Join-Path $sourceDir "include"
$env:SCALPEL_MUPDF_LIB_DIR = $libDir
if (Test-Path -LiteralPath $mutool) {
    $env:SCALPEL_MUTOOL_PATH = $mutool
}

Write-GeneratedFile $envFile (($psEnvLines -join "`r`n") + "`r`n")
Write-GeneratedFile $cmdEnvFile (($cmdEnvLines -join "`r`n") + "`r`n")

Write-Host "MuPDF is ready."
Write-Host "Run in PowerShell: . $envFile"
Write-Host "Run in cmd.exe: call `"$cmdEnvFile`""
Write-Host "Build Scalpel from a VS x64 Developer shell, or run Cargo after vcvars64.bat."
