# Reads (and optionally sets) the Studio Display XDR backlight over HID, in nits.
#   brightness.ps1              -> print current value
#   brightness.ps1 -Nits 200    -> set, then print the value the display reports back
param([double]$Nits = -1)
$src = @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class XdrBrightness
{
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sa, uint disposition, uint flags, IntPtr template);
    [DllImport("hid.dll", SetLastError = true)] static extern bool HidD_GetFeature(SafeFileHandle h, byte[] buf, int len);
    [DllImport("hid.dll", SetLastError = true)] static extern bool HidD_SetFeature(SafeFileHandle h, byte[] buf, int len);
    static SafeFileHandle Open(string path) { return CreateFile(path, 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero); }
    public static double Get(string path)
    {
        using (SafeFileHandle h = Open(path))
        {
            if (h.IsInvalid) return -1;
            byte[] b = new byte[7]; b[0] = 1;
            if (!HidD_GetFeature(h, b, b.Length)) return -2;
            return BitConverter.ToUInt32(b, 1) / 100.0;
        }
    }
    public static bool Set(string path, double nits)
    {
        using (SafeFileHandle h = Open(path))
        {
            if (h.IsInvalid) return false;
            byte[] b = new byte[7]; b[0] = 1;
            BitConverter.GetBytes((uint)Math.Round(nits * 100)).CopyTo(b, 1);
            return HidD_SetFeature(h, b, b.Length);
        }
    }
}
'@
Add-Type -TypeDefinition $src
$dev = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object { $_.InstanceId -like 'HID\VID_05AC&PID_1116&MI_07&COL01\*' } | Select-Object -First 1
if (-not $dev) { 'Studio Display XDR brightness interface not found'; exit 1 }
$path = '\\?\' + $dev.InstanceId.Replace('\', '#') + '#{4d1e55b2-f16f-11cf-88cb-001111000030}'
"current = $([XdrBrightness]::Get($path)) nits"
if ($Nits -ge 0) {
    "set $Nits -> $([XdrBrightness]::Set($path, $Nits))"
    Start-Sleep -Milliseconds 500
    "now     = $([XdrBrightness]::Get($path)) nits"
}
