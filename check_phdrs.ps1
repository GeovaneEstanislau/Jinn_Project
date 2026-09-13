$elf = [System.IO.File]::ReadAllBytes('target\x86_64-unknown-none\release\jinn')
$phoff = [System.BitConverter]::ToUInt64($elf, 32)
$phnum = [System.BitConverter]::ToUInt16($elf, 56)

Write-Host "Program Headers ($phnum):"
for ($i = 0; $i -lt $phnum; $i++) {
    $offset = $phoff + $i * 56
    $type = [System.BitConverter]::ToUInt32($elf, $offset)
    $flags = [System.BitConverter]::ToUInt32($elf, $offset + 4)
    $p_offset = [System.BitConverter]::ToUInt64($elf, $offset + 8)
    $p_vaddr = [System.BitConverter]::ToUInt64($elf, $offset + 16)
    $p_filesz = [System.BitConverter]::ToUInt64($elf, $offset + 32)
    $p_memsz = [System.BitConverter]::ToUInt64($elf, $offset + 40)
    
    Write-Host ("  Type: $type, Flags: $flags, Offset: 0x{0:X}, VAddr: 0x{1:X}, FileSz: 0x{2:X}, MemSz: 0x{3:X}" -f $p_offset, $p_vaddr, $p_filesz, $p_memsz)
}
