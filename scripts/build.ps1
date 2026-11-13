# build.ps1 - Build the kernel and copy ELF to boot folder
$ErrorActionPreference = 'Stop'

# Move to the project root (one level up from the scripts folder)
Set-Location -Path (Join-Path $PSScriptRoot "..")

# Ensure cargo is in PATH if installed in user profile
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path $cargoBin) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

# Build release ELF for the official bare-metal target
& cargo build --release --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) {
    throw "Kernel build failed with exit code $LASTEXITCODE. Existing artifacts were not updated."
}

# Ensure the boot directory exists (relative to the project root)
$bootDir = Join-Path -Path (Get-Location) "boot"
New-Item -ItemType Directory -Force -Path $bootDir | Out-Null

# Copy the produced ELF to boot/kernel.elf and iso_root/boot/jinn_kernel
$elfPath = Join-Path -Path (Join-Path "target" "x86_64-unknown-none\release") "jinn"
if (-not (Test-Path $elfPath)) {
    throw "Kernel ELF not found after successful build: $elfPath"
}
Copy-Item -Path $elfPath -Destination (Join-Path $bootDir "kernel.elf") -Force

$isoBootDir = Join-Path -Path (Get-Location) "iso_root\boot"
New-Item -ItemType Directory -Force -Path $isoBootDir | Out-Null
Copy-Item -Path $elfPath -Destination (Join-Path $isoBootDir "kernel.elf") -Force

# --- Build User Apps with Ring 3 canonical addressing (>= 0x10000) ---
$origRustflags = $env:RUSTFLAGS
$env:RUSTFLAGS = "-C link-arg=-no-pie -C link-arg=-Ttext=0x400000"

# --- Build ola-mundo ---
Write-Host "Building ola-mundo app..."
Push-Location "apps\ola-mundo"
& cargo build --release --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    $env:RUSTFLAGS = $origRustflags
    throw "App ola-mundo build failed."
}
Pop-Location

$appElfPath = "apps\ola-mundo\target\x86_64-unknown-none\release\ola-mundo"
if (-not (Test-Path $appElfPath)) {
    $appElfPath = "apps\ola-mundo\target\x86_64-jinn-user\release\ola-mundo"
}
if (-not (Test-Path $appElfPath)) {
    throw "App ELF not found: $appElfPath"
}

$isoAppsDir = Join-Path -Path (Get-Location) "iso_root\apps"
New-Item -ItemType Directory -Force -Path $isoAppsDir | Out-Null
Copy-Item -Path $appElfPath -Destination (Join-Path $isoAppsDir "ola-mundo.elf") -Force
Copy-Item -Path $appElfPath -Destination (Join-Path $isoAppsDir "browser.elf") -Force

# --- Build ola-grafico ---
Write-Host "Building ola-grafico app..."
Push-Location "apps\ola-grafico"
& cargo build --release --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "App ola-grafico build failed."
}
Pop-Location

$grafElfPath = "apps\ola-grafico\target\x86_64-unknown-none\release\ola-grafico"
if (-not (Test-Path $grafElfPath)) {
    $grafElfPath = "apps\ola-grafico\target\x86_64-jinn-user\release\ola-grafico"
}
if (-not (Test-Path $grafElfPath)) {
    throw "App ola-grafico ELF not found: $grafElfPath"
}
Copy-Item -Path $grafElfPath -Destination (Join-Path $isoAppsDir "ola-grafico.elf") -Force

# --- Build contador ---
Write-Host "Building contador app..."
Push-Location "apps\contador"
& cargo build --release --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "App contador build failed."
}
Pop-Location

$contElfPath = "apps\contador\target\x86_64-unknown-none\release\contador"
if (-not (Test-Path $contElfPath)) {
    $contElfPath = "apps\contador\target\x86_64-jinn-user\release\contador"
}
if (-not (Test-Path $contElfPath)) {
    throw "App contador ELF not found: $contElfPath"
}
Copy-Item -Path $contElfPath -Destination (Join-Path $isoAppsDir "contador.elf") -Force

# --- Build aladdin-server (Ring 3 Graphics Display Server) ---
Write-Host "Building aladdin-server (Ring 3 Graphics Service)..."
Push-Location "apps\aladdin-server"
& cargo build --release --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    throw "Aladdin Server build failed."
}
Pop-Location

$serverElfPath = "apps\aladdin-server\target\x86_64-unknown-none\release\aladdin-server"
if (-not (Test-Path $serverElfPath)) {
    $serverElfPath = "apps\aladdin-server\target\x86_64-jinn-user\release\aladdin-server"
}
if (-not (Test-Path $serverElfPath)) {
    throw "Aladdin Server ELF not found: $serverElfPath"
}
Copy-Item -Path $serverElfPath -Destination (Join-Path $isoAppsDir "aladdin-server.elf") -Force
Copy-Item -Path $serverElfPath -Destination (Join-Path $isoAppsDir "aladdin.elf") -Force
$env:RUSTFLAGS = $origRustflags

# --- Gera imagem de disco virtual FAT32 com MBR ---
$makeDiskScript = Join-Path -Path (Get-Location) "scripts\make_disk.ps1"
if (Test-Path $makeDiskScript) {
    Write-Host "Gerando imagem de disco virtual FAT32 (disk.img)..."
    & powershell -NoProfile -ExecutionPolicy Bypass -File $makeDiskScript
}

Write-Host "Build completed. Kernel, Apps e Disco FAT32 prontos!"
