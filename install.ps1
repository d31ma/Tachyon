# Tachyon installer for Windows.
#   irm https://tachyon.del.ma/install.ps1 | iex
# Downloads the latest `ty` Windows binary from GitHub releases, verifies its
# checksum, installs it under %LOCALAPPDATA%\Tachyon (added to your user PATH),
# then installs the `fylo` and `chex` binaries Tachyon drives at runtime.
$ErrorActionPreference = 'Stop'

$repo = 'd31ma/Tachyon'
$base = "https://github.com/$repo/releases/latest/download"
$asset = 'ty-windows-x64.exe'

$dest = Join-Path $env:LOCALAPPDATA 'Tachyon'
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$exe = Join-Path $dest 'ty.exe'

Write-Host "Downloading $asset..."
Invoke-WebRequest -Uri "$base/$asset" -OutFile $exe

# Verify checksum (best-effort).
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

# Add install dir to the user PATH if it isn't already there.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$dest*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User')
    Write-Host "Added $dest to your user PATH (restart your terminal to pick it up)."
}

Write-Host "Installed ty to $exe"

# Tachyon drives the FYLO and CHEX binaries at runtime — install them too.
Write-Host "Installing runtime dependencies (fylo, chex)..."
irm https://github.com/d31ma/Fylo/releases/latest/download/install.ps1 | iex
irm https://github.com/d31ma/Chex/releases/latest/download/install.ps1 | iex

Write-Host "Run 'ty --help' to get started."
