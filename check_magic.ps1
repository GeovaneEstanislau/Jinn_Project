$elf = [System.IO.File]::ReadAllBytes("target\x86_64-unknown-none\release\jinn")
Write-Host ("ELF size: " + $elf.Length + " bytes")

$magic0 = [byte[]]@(0x88, 0x8B, 0x4C, 0xDF, 0x30, 0xDD, 0xB1, 0xC7)
$magic1 = [byte[]]@(0x4E, 0x22, 0x5E, 0x44, 0xAB, 0x1E, 0x69, 0x0A)
$base0  = [byte[]]@(0xC8, 0xA6, 0x95, 0x5C, 0x2D, 0x2B, 0x56, 0xF9)

$c0 = 0
$c1 = 0
$cb = 0

for ($i = 0; $i -le $elf.Length - 8; $i++) {
    $ok0 = $true
    $ok1 = $true
    $okb = $true
    
    for ($j = 0; $j -lt 8; $j++) {
        if ($elf[$i+$j] -ne $magic0[$j]) { $ok0 = $false }
        if ($elf[$i+$j] -ne $magic1[$j]) { $ok1 = $false }
        if ($elf[$i+$j] -ne $base0[$j]) { $okb = $false }
    }
    
    if ($ok0) { $c0++ }
    if ($ok1) { $c1++ }
    if ($okb) { $cb++ }
}

Write-Host ("COMMON_MAGIC_0 found: " + $c0)
Write-Host ("COMMON_MAGIC_1 found: " + $c1)
Write-Host ("BASE_REVISION found: " + $cb)
