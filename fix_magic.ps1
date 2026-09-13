$content = Get-Content src\limine.rs -Raw
$content = $content -replace '0x0a691eab445e224e', '0x0a82e883a194f07b'
$content = $content -replace '0x9d5827dcd881dd75, 0xa3148604f34c6e58', '0x9d5827dcd881dd75, 0xa3148604f6fab11b'
$content = $content -replace '0xf55027d8e1aa3614, 0xed20ea8d1de3fccd', '0xf55038d8e2a1202f, 0x279426fcf5f59740'
$content = $content -replace '0x48d564a7d6e96b29, 0x6a0f88305b68ae17', '0x48dcf1cb8ad2b852, 0x63984e959a98244b'
$content = $content -replace '0x67cf3d9d378a806f, 0xe304acdfc50c3c62', '0x67cf3d9d378a806f, 0xe304acdfc50c3c62' # (already fixed this one previously, just to be sure)
Set-Content src\limine.rs -Value $content
