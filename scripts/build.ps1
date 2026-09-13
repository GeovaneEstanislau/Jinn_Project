
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

# Build release ELF for the official bare-metal target using the locally installed rust-src.
& cargo build --release --target x86_64-unknown-none '-Zbuild-std=core,alloc'
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

Write-Host "Build completed. ELF copied to $bootDir\kernel.elf and $isoBootDir\kernel.elf"
