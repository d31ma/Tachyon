# Tachyon installer for Windows.
#   irm https://tachyon.del.ma/install.ps1 | iex
# Downloads the latest `ty` Windows binary from GitHub releases, verifies its
# checksum, installs it under %LOCALAPPDATA%\Tachyon (added to your user PATH),
# then installs the `fylo`, `chex`, and `ttid` binaries Tachyon drives at runtime.
$ErrorActionPreference = 'Stop'

$repo = 'd31ma/Tachyon'
$base = "https://github.com/$repo/releases/latest/download"
$asset = 'ty-windows-x64.exe'
$script:TachyonSteps = 8
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

$dest = Join-Path $env:LOCALAPPDATA 'Tachyon'
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$exe = Join-Path $dest 'ty.exe'
Write-TachyonStep "Selected install directory: $dest"

Write-TachyonStep "Downloading $asset"
Invoke-WebRequest -Uri "$base/$asset" -OutFile $exe

# Verify checksum (best-effort).
Write-TachyonStep 'Verifying release checksum'
try {
    $sums = (Invoke-WebRequest -Uri "$base/SHA256SUMS" -UseBasicParsing).Content
    $line = ($sums -split "`n") | Where-Object { $_ -match "\s$([regex]::Escape($asset))$" } | Select-Object -First 1
    if ($line) {
        $expected = ($line -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLower()
        if ($expected -ne $actual) {
            Remove-Item $exe -Force
            throw "Checksum mismatch for $asset. Aborting."
        }
    }
} catch {
    Write-Warning "Could not verify checksum: $_"
}

Write-TachyonStep 'Installing ty'

# Add install dir to the user PATH if it isn't already there.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User')
    Write-Host "Added $dest to your user PATH (restart your terminal to pick it up)."
}

Write-Host "Installed ty to $exe"

# Tachyon drives the FYLO, CHEX, and TTID binaries at runtime — install them too.
Write-TachyonStep 'Installing FYLO runtime'
irm https://github.com/d31ma/Fylo/releases/latest/download/install.ps1 | iex
Write-TachyonStep 'Installing CHEX validator'
irm https://github.com/d31ma/Chex/releases/latest/download/install.ps1 | iex
Write-TachyonStep 'Installing TTID generator'
irm https://github.com/d31ma/TTID/releases/latest/download/install.ps1 | iex

Write-Progress -Activity 'Tachyon install' -Completed
Write-Host "Run 'ty --help' to get started."
