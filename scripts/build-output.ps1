$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$output = Join-Path $repo 'output'
$edge = Join-Path $output 'edge-extension'
$windows = Join-Path $output 'windows'
$windowsEdge = Join-Path $windows 'edge-extension'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path -LiteralPath $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

Push-Location $repo
try {
  & npx tauri build --no-bundle
  if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
  & cargo build --release --manifest-path 'src-tauri/Cargo.toml' --bin aurum-native-host
  if ($LASTEXITCODE -ne 0) { throw 'Native host build failed.' }

  New-Item -ItemType Directory -Path $edge, $windows, $windowsEdge -Force | Out-Null
  foreach ($name in @('manifest.json', 'background.js', 'popup.html', 'popup.js', 'icon128.png')) {
    Copy-Item -LiteralPath (Join-Path $repo "browser-extension/edge/$name") -Destination (Join-Path $edge $name) -Force
    Copy-Item -LiteralPath (Join-Path $repo "browser-extension/edge/$name") -Destination (Join-Path $windowsEdge $name) -Force
  }
  Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/target/release/aurum-download-manager.exe') -Destination $windows -Force
  Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/target/release/aurum-native-host.exe') -Destination $windows -Force
  $aria2Command = Get-Command 'aria2c.exe' -ErrorAction Stop
  $aria2Item = Get-Item -LiteralPath $aria2Command.Source
  $aria2Binary = if ($aria2Item.LinkType -eq 'SymbolicLink') { $aria2Item.Target } else { $aria2Item.FullName }
  $aria2Version = (& $aria2Binary --version | Select-Object -First 1)
  if ($aria2Version -ne 'aria2 version 1.37.0') { throw "Expected aria2 1.37.0, found: $aria2Version" }
  $aria2License = Join-Path (Split-Path $aria2Binary -Parent) 'COPYING'
  if (-not (Test-Path -LiteralPath $aria2License)) { throw 'aria2 COPYING file was not found.' }
  Copy-Item -LiteralPath $aria2Binary -Destination (Join-Path $windows 'aria2c.exe') -Force
  Copy-Item -LiteralPath $aria2License -Destination (Join-Path $windows 'aria2-COPYING.txt') -Force
  $sourceArchive = Join-Path $output 'aria2-source-1.37.0.zip'
  if (-not (Test-Path -LiteralPath $sourceArchive)) {
    & curl.exe -L --fail --retry 2 --output $sourceArchive 'https://github.com/aria2/aria2/archive/refs/tags/release-1.37.0.zip'
    if ($LASTEXITCODE -ne 0) { throw 'Could not download aria2 corresponding source.' }
  }
  $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceArchive).Hash
  if ($sourceHash -ne 'F0458EED12529086260C40B5054EE5D2FCF762B2B9DBE18487F56EEE6153DF7D') { throw 'Unexpected aria2 source archive hash.' }
  Copy-Item -LiteralPath $sourceArchive -Destination (Join-Path $windows 'aria2-source-1.37.0.zip') -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/install-edge-bridge.ps1') -Destination $windows -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/output-readme.md') -Destination (Join-Path $output 'README.md') -Force
  Copy-Item -LiteralPath (Join-Path $repo 'scripts/output-readme.md') -Destination (Join-Path $windows 'README.md') -Force

  $edgeFiles = @('manifest.json', 'background.js', 'popup.html', 'popup.js', 'icon128.png') | ForEach-Object { Join-Path $edge $_ }
  $windowsFiles = @('aurum-download-manager.exe', 'aurum-native-host.exe', 'aria2c.exe', 'aria2-COPYING.txt', 'aria2-source-1.37.0.zip', 'install-edge-bridge.ps1', 'README.md', 'edge-extension') | ForEach-Object { Join-Path $windows $_ }
  Compress-Archive -LiteralPath $edgeFiles -DestinationPath (Join-Path $output 'aurum-edge-extension.zip') -Force
  Compress-Archive -LiteralPath $windowsFiles -DestinationPath (Join-Path $output 'aurum-windows-portable.zip') -Force
  Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $output 'aurum-edge-extension.zip'), (Join-Path $output 'aurum-windows-portable.zip') |
    ForEach-Object { "$($_.Hash.ToLowerInvariant())  $(Split-Path $_.Path -Leaf)" } |
    Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
  Write-Output "Build output: $output"
} finally { Pop-Location }
