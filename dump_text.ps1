$elf = [System.IO.File]::ReadAllBytes('target\x86_64-unknown-none\release\jinn')
# The .text section is at Offset 0x1000 and has size 0x113 (from the program headers)
$text = New-Object byte[] 0x113
[Array]::Copy($elf, 0x1000, $text, 0, 0x113)
$hex = [System.BitConverter]::ToString($text) -replace '-', ' '
Write-Host "Disassembly of .text:"
Write-Host $hex
