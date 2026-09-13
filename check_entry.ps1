$elf = [System.IO.File]::ReadAllBytes('target\x86_64-unknown-none\release\jinn')
$e_entry = [System.BitConverter]::ToUInt64($elf, 24)
Write-Host ('ELF Entry Point: 0x{0:X}' -f $e_entry)
