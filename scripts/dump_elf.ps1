$files = @("iso_root\apps\ola-grafico.elf", "iso_root\apps\contador.elf", "iso_root\apps\ola-mundo.elf")
foreach ($path in $files) {
    if (-not (Test-Path $path)) { continue }
    $b = [System.IO.File]::ReadAllBytes($path)
    $entry = [BitConverter]::ToUInt64($b, 24)
    $phoff = [BitConverter]::ToUInt64($b, 32)
    $phnum = [BitConverter]::ToUInt16($b, 56)
    Write-Host ("=== {0} entry=0x{1:X} phoff={2} phnum={3}" -f $path, $entry, $phoff, $phnum)
    for ($i = 0; $i -lt $phnum; $i++) {
        $off = $phoff + $i * 56
        $type = [BitConverter]::ToUInt32($b, $off)
        $flags = [BitConverter]::ToUInt32($b, $off + 4)
        $offset = [BitConverter]::ToUInt64($b, $off + 8)
        $vaddr = [BitConverter]::ToUInt64($b, $off + 16)
        $filesz = [BitConverter]::ToUInt64($b, $off + 32)
        $memsz = [BitConverter]::ToUInt64($b, $off + 40)
        $align = [BitConverter]::ToUInt64($b, $off + 48)
        Write-Host ("  PH[{0}] type={1} flags={2} offset=0x{3:X} vaddr=0x{4:X} filesz=0x{5:X} memsz=0x{6:X} align=0x{7:X}" -f $i, $type, $flags, $offset, $vaddr, $filesz, $memsz, $align)
    }
}
