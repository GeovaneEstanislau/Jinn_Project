$b = [System.IO.File]::ReadAllBytes('apps\ola-mundo\target\x86_64-jinn-user\release\ola-mundo')
$phoff = [BitConverter]::ToUInt64($b, 32)
$phnum = [BitConverter]::ToUInt16($b, 56)
Write-Host "phoff=$phoff phnum=$phnum"
for ($i=0; $i -lt $phnum; $i++) {
    $off = $phoff + $i*56
    $type = [BitConverter]::ToUInt32($b, $off)
    $flags = [BitConverter]::ToUInt32($b, $off+4)
    $vaddr = [BitConverter]::ToUInt64($b, $off+16)
    $filesz = [BitConverter]::ToUInt64($b, $off+32)
    $memsz = [BitConverter]::ToUInt64($b, $off+40)
    Write-Host ("PH[{0}] type={1} flags={2} vaddr=0x{3:X} filesz={4} memsz={5}" -f $i, $type, $flags, $vaddr, $filesz, $memsz)
}
