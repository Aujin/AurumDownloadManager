$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$output = Join-Path $repo 'output'
$edge = Join-Path $output 'edge-extension'
$windows = Join-Path $output 'windows'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path -LiteralPath $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

Push-Location $repo
try {
  & npm run build
  if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
  & cargo build --release --manifest-path 'src-tauri/Cargo.toml' --bins
  if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }

  New-Item -ItemType Directory -Path $edge, $windows -Force | Out-Null
  foreach ($name in @('manifest.json', 'background.js', 'popup.html', 'popup.js', 'icon128.png')) {
    Copy-Item -LiteralPath (Join-Path $repo "browser-extension/edge/$name") -Destination (Join-Path $edge $name) -Force
  }
  Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/target/release/aurum-download-manager.exe') -Destination $windows -Force
  Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/target/release/aurum-native-host.exe') -Destination $windows -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/install-edge-bridge.ps1') -Destination $windows -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/output-readme.md') -Destination (Join-Path $output 'README.md') -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/output-readme.md') -Destination (Join-Path $windows 'README.md') -Force

  $edgeFiles = @('manifest.json', 'background.js', 'popup.html', 'popup.js', 'icon128.png') | ForEach-Object { Join-Path $edge $_ }
  $windowsFiles = @('aurum-download-manager.exe', 'aurum-native-host.exe', 'install-edge-bridge.ps1', 'README.md') | ForEach-Object { Join-Path $windows $_ }
  Compress-Archive -LiteralPath $edgeFiles -DestinationPath (Join-Path $output 'aurum-edge-extension.zip') -Force
  Compress-Archive -LiteralPath $windowsFiles -DestinationPath (Join-Path $output 'aurum-windows-portable.zip') -Force
  Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $output 'aurum-edge-extension.zip'), (Join-Path $output 'aurum-windows-portable.zip') |
    ForEach-Object { "$($_.Hash.ToLowerInvariant())  $(Split-Path $_.Path -Leaf)" } |
    Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
  Write-Output "Build output: $output"
} finally { Pop-Location }
