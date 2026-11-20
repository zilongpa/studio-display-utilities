# Binds the Studio Display XDR's sensor interfaces to WinUSB, which is what Zadig does by hand.
# Tested on Windows 11 25H2 (26200): both interfaces came up with WinUSB and the program read the sensors.
#
#   Run from an elevated PowerShell:
#     powershell -ExecutionPolicy Bypass -File install-sensor-driver.ps1               ambient light sensor (interface 8)
#     powershell -ExecutionPolicy Bypass -File install-sensor-driver.ps1 -Orientation  also the orientation sensor (interface 9)
#     powershell -ExecutionPolicy Bypass -File install-sensor-driver.ps1 -Remove       undo: remove the driver package and the certificate
#
# Why this is needed: Windows' own HID driver fails to start on these interfaces (Code 10), so they are
# given to WinUSB, Microsoft's generic USB driver that ships with Windows, and read by the program.
#
# What it changes on the machine, and why:
# - Windows only installs a driver package whose catalogue is signed by a publisher it trusts. The
#   package here contains no driver code of its own (only a text file naming Microsoft's WinUSB), but
#   it still needs that signature. So the script creates a signing certificate just for this, puts its
#   public half into the machine's "Trusted Root" and "Trusted Publishers" stores, signs the catalogue,
#   and then destroys the private key, so nothing else can ever be signed with it.
# - It adds the driver package to the driver store and installs it on the interface(s).
param([switch]$Orientation, [switch]$Remove)
$ErrorActionPreference = 'Stop'
$subject = 'CN=Studio Display Utilities Local Driver Signing'
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw 'Run this from an elevated PowerShell (Run as administrator).' }

if ($Remove) {
    $packages = pnputil /enum-drivers | Out-String
    foreach ($match in [regex]::Matches($packages, '(oem\d+\.inf)[^\r\n]*\r?\n[^\r\n]*sdu-winusb\.inf')) {
        "removing driver package $($match.Groups[1].Value)"
        pnputil /delete-driver $match.Groups[1].Value /uninstall /force
    }
    foreach ($store in 'Root', 'TrustedPublisher') {
        Get-ChildItem "Cert:\LocalMachine\$store" | Where-Object Subject -like 'CN=Studio Display Utilities Local Driver Signing' | ForEach-Object { "removing certificate from $store"; Remove-Item $_.PSPath }
    }
    'Done. Unplug the display and plug it in again; the interfaces go back to Code 10.'
    return
}

$folder = Join-Path $env:TEMP "sdu-winusb-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $folder | Out-Null
$devices = @('%Light%=WinUsb, USB\VID_05AC&PID_1116&MI_08')
if ($Orientation) { $devices += '%Turn%=WinUsb, USB\VID_05AC&PID_1116&MI_09' }
$inf = @"
; Gives the Studio Display XDR's sensor interfaces to WinUSB. Contains no driver code.
[Version]
Signature   = "`$Windows NT`$"
Class       = USBDevice
ClassGUID   = {88BAE032-5A81-49f0-BC3D-A4FF138216D6}
Provider    = %Provider%
CatalogFile = sdu-winusb.cat
DriverVer   = $(Get-Date -Format 'MM/dd/yyyy'),1.0.0.0
PnpLockdown = 1

[Manufacturer]
%Provider% = Devices, NTamd64, NTarm64

[Devices.NTamd64]
$($devices -join "`r`n")

[Devices.NTarm64]
$($devices -join "`r`n")

[WinUsb]
Include = winusb.inf
Needs   = WINUSB.NT

[WinUsb.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WinUsb.HW]
AddReg = Interface

[Interface]
HKR,,DeviceInterfaceGUIDs,0x10000,"{6f0a4f0c-6c5b-4a57-9d8a-5d2f6c1e0a11}"

[Strings]
Provider = "Studio Display Utilities"
Light    = "Studio Display ambient light sensor"
Turn     = "Studio Display orientation sensor"
"@
Set-Content (Join-Path $folder 'sdu-winusb.inf') $inf -Encoding ascii
$catalogue = Join-Path $folder 'sdu-winusb.cat'
New-FileCatalog -Path (Join-Path $folder 'sdu-winusb.inf') -CatalogFilePath $catalogue -CatalogVersion 2.0 | Out-Null

'creating a signing certificate, used once'
$certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject -CertStoreLocation Cert:\LocalMachine\My -NotAfter (Get-Date).AddYears(10)
try {
    $public = Join-Path $folder 'sdu.cer'
    Export-Certificate -Cert $certificate -FilePath $public | Out-Null
    Import-Certificate -FilePath $public -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
    Import-Certificate -FilePath $public -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
    $signature = Set-AuthenticodeSignature -FilePath $catalogue -Certificate $certificate -HashAlgorithm SHA256
    if ($signature.Status -ne 'Valid') { throw "the catalogue could not be signed: $($signature.StatusMessage)" }
} finally {
    # the private key goes, so the certificate can never sign anything else
    Remove-Item $certificate.PSPath -DeleteKey
}
'installing the driver package'
pnputil /add-driver (Join-Path $folder 'sdu-winusb.inf') /install
Remove-Item $folder -Recurse -Force
'Done. If Device Manager still shows the interface with a warning sign, unplug the display and plug it in again.'
