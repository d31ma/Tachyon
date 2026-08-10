# Tachyon installer for Windows.
#   irm https://tachyon.del.ma/install.ps1 | iex
# Downloads the latest `ty` Windows binary from GitHub releases, verifies its
# checksum, then installs it under %LOCALAPPDATA%\Tachyon and adds that directory
# to your user PATH.
$ErrorActionPreference = 'Stop'

$repo = 'd31ma/Tachyon'
$base = if ($env:TACHYON_BASE_URL) { $env:TACHYON_BASE_URL } else { "https://github.com/$repo/releases/latest/download" }
$asset = 'ty-windows-x64.exe'
$script:TachyonSteps = 5
$script:TachyonStep = 0

function Write-TachyonStep {
    param([string]$Message)
    $script:TachyonStep += 1
    $percent = [Math]::Floor(($script:TachyonStep * 100) / $script:TachyonSteps)
    $filled = [Math]::Floor(($script:TachyonStep * 24) / $script:TachyonSteps)
    $empty = 24 - $filled
    $bar = ('#' * $filled) + ('-' * $empty)
    Write-Host ("TACHYON [{0}] {1,3}%  {2}" -f $bar, $percent, $Message)
    Write-Progress -Activity 'Tachyon install' -Status $Message -PercentComplete $percent
}

Write-Host 'TACHYON installer'
Write-Host 'Bringing the ty binary online...'
Write-Host ''

Write-TachyonStep "Detected windows/x64"

$dest = if ($env:TACHYON_INSTALL_DIR) { $env:TACHYON_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Tachyon' }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$exe = Join-Path $dest 'ty.exe'
$download = Join-Path $dest ("ty-{0}.download" -f [Guid]::NewGuid().ToString('N'))
$backup = Join-Path $dest ("ty-{0}.backup" -f [Guid]::NewGuid().ToString('N'))
Write-TachyonStep "Selected install directory: $dest"

try {
    Write-TachyonStep "Downloading $asset"
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $download

    # Verification is fail-closed: an unavailable checksum, a missing asset
    # entry, or a digest mismatch leaves any installed binary untouched.
    Write-TachyonStep 'Verifying release checksum'
    $sumsContent = (Invoke-WebRequest -Uri "$base/SHA256SUMS" -UseBasicParsing).Content
    $sums = if ($sumsContent -is [byte[]]) {
        [Text.Encoding]::UTF8.GetString($sumsContent)
    } else {
        [string]$sumsContent
    }
    $line = ($sums -split "`n") | Where-Object { $_.Trim() -match "^[0-9a-fA-F]{64}\s+$([regex]::Escape($asset))$" } | Select-Object -First 1
    if (-not $line) {
        throw "No checksum published for $asset. Aborting."
    }
    $expected = ($line.Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 $download).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "Checksum mismatch for $asset. Aborting."
    }

    Write-TachyonStep 'Installing ty'
    if (Test-Path $exe) {
        [IO.File]::Replace($download, $exe, $backup)
        Remove-Item $backup -Force
    } else {
        Move-Item -Path $download -Destination $exe
    }
} finally {
    Remove-Item $download -Force -ErrorAction SilentlyContinue
    Remove-Item $backup -Force -ErrorAction SilentlyContinue
}

# Add install dir to the user PATH if it isn't already there.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User')
    Write-Host "Added $dest to your user PATH (restart your terminal to pick it up)."
}

Write-Host "Installed ty to $exe"

Write-Progress -Activity 'Tachyon install' -Completed
Write-Host "Run 'ty --help' to get started."
