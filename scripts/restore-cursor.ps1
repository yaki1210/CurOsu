$ErrorActionPreference = "Stop"

Add-Type -Namespace CurosuRestore -Name Api -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true)]
public static extern System.IntPtr LoadCursor(System.IntPtr hInstance, System.IntPtr lpCursorName);

[DllImport("user32.dll", SetLastError = true)]
public static extern System.IntPtr CopyIcon(System.IntPtr hIcon);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool SetSystemCursor(System.IntPtr hcur, uint id);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, System.IntPtr pvParam, uint fWinIni);
'@

$ids = @(
    32512, 32513, 32514, 32515, 32516,
    32642, 32643, 32644, 32645, 32646,
    32648, 32649, 32650, 32651
)

foreach ($id in $ids)
{
    $original = [CurosuRestore.Api]::LoadCursor([System.IntPtr]::Zero, [System.IntPtr]$id)
    if ($original -ne [System.IntPtr]::Zero)
    {
        $copy = [CurosuRestore.Api]::CopyIcon($original)
        if ($copy -ne [System.IntPtr]::Zero)
        {
            [CurosuRestore.Api]::SetSystemCursor($copy, $id) | Out-Null
        }
    }
}

[CurosuRestore.Api]::SystemParametersInfo(0x0057, 0, [System.IntPtr]::Zero, 0x0002) | Out-Null
Write-Host "System cursors restored."