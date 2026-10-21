param([string]$ExtensionId = 'klblkidgakkmnemkgdhgdpjlhblbhhgb')

$ErrorActionPreference = 'Stop'
if ($ExtensionId -cnotmatch '^[a-p]{32}$') { throw 'Enter the 32-character ID shown on edge://extensions.' }
$hostBinary = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'aurum-native-host.exe')).Path
$dataDir = Join-Path $env:APPDATA 'com.aujin.aurumdownloadmanager'
New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
$manifestPath = Join-Path $dataDir 'native-host.json'
$manifest = [ordered]@{
  name = 'com.aujin.aurumdownloadmanager'
  description = 'Aurum Download Manager bridge'
  path = $hostBinary
  type = 'stdio'
  allowed_origins = @("chrome-extension://$ExtensionId/")
}
$json = $manifest | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($manifestPath, $json, (New-Object System.Text.UTF8Encoding($false)))
$registryPath = 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.aujin.aurumdownloadmanager'
New-Item -Path $registryPath -Force -Value $manifestPath | Out-Null
Write-Output "Aurum Edge bridge registered for extension ID: $ExtensionId"
