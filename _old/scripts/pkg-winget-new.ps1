param(
    [Parameter(Mandatory = $true)]
    [string]$Version  # e.g. 1.0.0
)

$PackageId    = "bgunnarsson.binsql"
$Owner        = "bgunnarsson"
$Repo         = "binsql"
$Tag          = $Version              # tag name in GitHub releases
$BinaryName   = "binsql-windows-amd64.exe"
$InstallerUrl = "https://github.com/$Owner/$Repo/releases/download/$Tag/$BinaryName"

Write-Host "Preparing WinGet NEW manifest for $PackageId $Version"
Write-Host "Installer URL: $InstallerUrl"

# Download installer and compute SHA256
$tempFile = New-TemporaryFile
try {
    Write-Host "Downloading installer to compute SHA256..."
    Invoke-WebRequest -Uri $InstallerUrl -OutFile $tempFile -UseBasicParsing
    $hash = (Get-FileHash $tempFile -Algorithm SHA256).Hash
    Write-Host "SHA256: $hash"
}
finally {
    if (Test-Path $tempFile) { Remove-Item $tempFile -Force }
}

# Create manifests
wingetcreate new `
    --publisher "Brynjólfur Gunnarsson" `
    --package-name "BINSQL" `
    --moniker "binsql" `
    --pkgid $PackageId `
    --version $Version `
    --url $InstallerUrl `
    --installer-type portable `
    --sha256 $hash `
    --license "MIT" `
    --short-description "Terminal UI for exploring SQL databases."

if ($LASTEXITCODE -ne 0) {
    Write-Error "wingetcreate new failed."
    exit 1
}

Write-Host ""
Write-Host "Manifests generated."

# Optional auto-submit if you export a GitHub token
if ($env:WINGET_GITHUB_TOKEN) {
    Write-Host "Submitting manifests using WINGET_GITHUB_TOKEN..."
    wingetcreate submit --token $env:WINGET_GITHUB_TOKEN
    if ($LASTEXITCODE -ne 0) {
        Write-Error "wingetcreate submit failed."
        exit 1
    }
    Write-Host "Submitted to microsoft/winget-pkgs."
} else {
    Write-Host ""
    Write-Host "No WINGET_GITHUB_TOKEN set."
    Write-Host "You can submit manually with:"
    Write-Host "  wingetcreate submit --token <YOUR_GITHUB_PAT>"
}

# usage:
# .\winget-new.ps1 -Version 1.0.0
