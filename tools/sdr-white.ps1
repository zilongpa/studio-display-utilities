# Reads (and optionally sets) Windows' "SDR content brightness" for the HDR display, in nits.
# This is what Settings > Display > HDR > "SDR content brightness" changes (80..480 nits).
#   sdr-white.ps1              -> print current value
#   sdr-white.ps1 -Nits 200    -> set, then print
# Setting uses an undocumented DisplayConfig request (type 0xFFFFFFEE, Windows 11 24H2+).
param([double]$Nits = -1)
$src = @'
using System;
using System.Runtime.InteropServices;
public static class SdrWhiteLevel
{
    [DllImport("user32.dll")] static extern int GetDisplayConfigBufferSizes(uint flags, out uint paths, out uint modes);
    [DllImport("user32.dll")] static extern int QueryDisplayConfig(uint flags, ref uint numPaths, byte[] paths, ref uint numModes, byte[] modes, IntPtr topology);
    [DllImport("user32.dll")] static extern int DisplayConfigGetDeviceInfo(byte[] packet);
    [DllImport("user32.dll")] static extern int DisplayConfigSetDeviceInfo(byte[] packet);
    static byte[] Packet(uint type, int size, int index)
    {
        uint np, nm; GetDisplayConfigBufferSizes(2, out np, out nm);
        byte[] paths = new byte[np * 72], modes = new byte[nm * 64];
        QueryDisplayConfig(2, ref np, paths, ref nm, modes, IntPtr.Zero);
        byte[] p = new byte[size];
        BitConverter.GetBytes(type).CopyTo(p, 0); BitConverter.GetBytes(size).CopyTo(p, 4);
        Array.Copy(paths, index * 72 + 20, p, 8, 8); Array.Copy(paths, index * 72 + 28, p, 16, 4);
        return p;
    }
    public static double Get()
    {
        byte[] p = Packet(11, 24, 0);
        return DisplayConfigGetDeviceInfo(p) == 0 ? BitConverter.ToUInt32(p, 20) * 80 / 1000.0 : -1;
    }
    public static int Set(double nits)
    {
        byte[] p = Packet(0xFFFFFFEE, 28, 0);
        BitConverter.GetBytes((uint)Math.Round(nits * 1000 / 80)).CopyTo(p, 20);
        p[24] = 1; // finalValue
        return DisplayConfigSetDeviceInfo(p);
    }
}
'@
Add-Type -TypeDefinition $src
"current = $([SdrWhiteLevel]::Get()) nits"
if ($Nits -ge 0) {
    "set $Nits -> ret=$([SdrWhiteLevel]::Set($Nits))"
    Start-Sleep -Milliseconds 500
    "now     = $([SdrWhiteLevel]::Get()) nits"
}
