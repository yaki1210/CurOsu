param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\curosu.exe"),
    [string]$InstallDir = "C:\Program Files\OsuCursorRs"
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))
{
    Write-Error "This script must be run as administrator."
    exit 1
}

& (Join-Path $PSScriptRoot "stop-running.ps1")

$sourceExe = (Resolve-Path -LiteralPath $ExePath).Path
$certSubject = "CN=Osu Cursor Local"
$cert = Get-ChildItem "Cert:\CurrentUser\My" |
    Where-Object {
        $_.Subject -eq $certSubject -and
        ($_.EnhancedKeyUsageList.ObjectId -contains "1.3.6.1.5.5.7.3.3")
    } |
    Select-Object -First 1

if (-not $cert)
{
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $certSubject `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -KeyExportPolicy Exportable `
        -NotAfter (Get-Date).AddYears(10)
}

$certDir = Join-Path $PSScriptRoot "..\certs"
New-Item -ItemType Directory -Force -Path $certDir | Out-Null
$certPath = Join-Path $certDir "OsuCursorLocal.cer"
Export-Certificate -Cert $cert -FilePath $certPath -Force | Out-Null

$rootCert = Get-ChildItem "Cert:\LocalMachine\Root" | Where-Object Thumbprint -eq $cert.Thumbprint
if (-not $rootCert)
{
    Import-Certificate -FilePath $certPath -CertStoreLocation "Cert:\LocalMachine\Root" | Out-Null
}

$publisherCert = Get-ChildItem "Cert:\LocalMachine\TrustedPublisher" | Where-Object Thumbprint -eq $cert.Thumbprint
if (-not $publisherCert)
{
    Import-Certificate -FilePath $certPath -CertStoreLocation "Cert:\LocalMachine\TrustedPublisher" | Out-Null
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$destExe = Join-Path $InstallDir "curosu.exe"
Copy-Item -LiteralPath $sourceExe -Destination $destExe -Force
Set-AuthenticodeSignature -FilePath $destExe -Certificate $cert -HashAlgorithm SHA256 | Out-Null

$signature = Get-AuthenticodeSignature -FilePath $destExe
if ($signature.Status -ne "Valid")
{
    throw "Signature verification failed: $($signature.Status)"
}

Write-Host "Installed: $destExe"
Write-Host "Signature status: $($signature.Status)"
Write-Host "Launch it from this secure location so UIAccess can overlay immersive shell windows."