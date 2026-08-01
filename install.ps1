# Tachyon installer for Windows.
#   irm https://tachyon.del.ma/install.ps1 | iex
# Downloads the latest `ty` Windows binary from GitHub releases, verifies its
# checksum, installs it under %LOCALAPPDATA%\Tachyon (added to your user PATH),
# then installs the `chex` and `ttid` binaries Tachyon drives at runtime.
$ErrorActionPreference = 'Stop'

$repo = 'd31ma/Tachyon'
$base = if ($env:TACHYON_BASE_URL) { $env:TACHYON_BASE_URL } else { "https://github.com/$repo/releases/latest/download" }
$asset = 'ty-windows-x64.exe'
$script:TachyonSteps = 7
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
Write-TachyonStep "Selected install directory: $dest"

Write-TachyonStep "Downloading $asset"
Invoke-WebRequest -Uri "$base/$asset" -OutFile $exe

# Verification is fail-closed: an unavailable checksum, a missing asset entry,
# or a digest mismatch aborts the install.
Write-TachyonStep 'Verifying release checksum'
$sums = (Invoke-WebRequest -Uri "$base/SHA256SUMS" -UseBasicParsing).Content
$line = ($sums -split "`n") | Where-Object { $_.Trim() -match "^[0-9a-fA-F]{64}\s+$([regex]::Escape($asset))$" } | Select-Object -First 1
if (-not $line) {
    Remove-Item $exe -Force
    throw "No checksum published for $asset. Aborting."
}
$expected = ($line.Trim() -split '\s+')[0].ToLower()
$actual = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLower()
if ($expected -ne $actual) {
    Remove-Item $exe -Force
    throw "Checksum mismatch for $asset. Aborting."
}

Write-TachyonStep 'Installing ty'

# Add install dir to the user PATH if it isn't already there.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User')
    Write-Host "Added $dest to your user PATH (restart your terminal to pick it up)."
}

Write-Host "Installed ty to $exe"

# Release CI skips optional tools so it can test this installer hermetically.
if ($env:TACHYON_SKIP_OPTIONAL_TOOLS -eq '1') {
    Write-TachyonStep 'Skipping optional CHEX validator'
    Write-TachyonStep 'Skipping optional TTID generator'
} else {
    Write-TachyonStep 'Installing CHEX validator'
    irm https://github.com/d31ma/Chex/releases/latest/download/install.ps1 | iex
    Write-TachyonStep 'Installing TTID generator'
    irm https://github.com/d31ma/TTID/releases/latest/download/install.ps1 | iex
}

Write-Progress -Activity 'Tachyon install' -Completed
Write-Host "Run 'ty --help' to get started."
