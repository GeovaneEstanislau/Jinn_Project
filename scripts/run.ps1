# run.ps1 - Gera a ISO com Limine e inicia o QEMU
param(
    [switch]$NoRun
)

$ErrorActionPreference = 'Stop'

# Força TLS 1.2 (necessário para conexões com GitHub no PowerShell antigo)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# ------------------------------------------------------------------
# 1) Caminho do projeto
# ------------------------------------------------------------------
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

# ------------------------------------------------------------------
# 1.5) Build do kernel (garante que a ISO sempre receba o binário mais
# recente)
# ------------------------------------------------------------------
$buildScript = Join-Path $projectRoot "scripts\build.ps1"
if (Test-Path $buildScript) {
    Write-Host "Executando build do kernel: $buildScript"
    # Executa o script de build. Ele já faz Set-Location para o root do projeto.
    & powershell -NoProfile -ExecutionPolicy Bypass -File $buildScript
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Falha no build do kernel. Abortando geração da ISO."
        exit 1
    }
} else {
    Write-Host "Aviso: script de build não encontrado em $buildScript. Pulando build."
}

# ------------------------------------------------------------------
# 2) Localizar ou baixar o binário limine-bios-x86_64.bin
# ------------------------------------------------------------------
$limineBin = $null
$limineCdBin = $null
$limineSys = $null
$bootx64Efi = $null

# Procura nos caminhos mais comuns
$candidates = @(
    (Join-Path $projectRoot "limine\limine-bios-x86_64.bin"),
    (Join-Path $projectRoot "limine\bin\limine-bios-x86_64.bin"),
    (Join-Path $projectRoot "limine-bios-x86_64.bin")
)
foreach ($c in $candidates) {
    if (Test-Path $c) {
        $limineBin = $c
        Write-Host "Limine BIOS binary encontrado: $limineBin"
        break
    }
}

$limineCdCandidates = @(
    (Join-Path $projectRoot "limine\usr\share\limine\limine-bios-cd.bin"),
    (Join-Path $projectRoot "limine\limine-bios-cd.bin")
)
$limineCdBin = $limineCdCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
$limineSysCandidates = @(
    (Join-Path $projectRoot "limine\usr\share\limine\limine-bios.sys"),
    (Join-Path $projectRoot "limine\limine-bios.sys")
)
$limineSys = $limineSysCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
$bootx64EfiCandidates = @(
    (Join-Path $projectRoot "limine\usr\share\limine\BOOTX64.EFI"),
    (Join-Path $projectRoot "limine\BOOTX64.EFI")
)
$bootx64Efi = $bootx64EfiCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

# Se não encontrou, baixa diretamente da release mais recente
if (-not $limineBin) {
    Write-Host "Binário não encontrado. Consultando a última release do Limine no GitHub..."
    $apiUrl  = "https://api.github.com/repos/limine-bootloader/limine/releases/latest"
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{'User-Agent' = 'PowerShell'}
    $tag     = $release.tag_name
    Write-Host "Última release: $tag"

    # URL direta do binário pré-compilado na release
    $binUrl  = "https://github.com/limine-bootloader/limine/releases/download/$tag/limine-bios-x86_64.bin"
    $destBin = Join-Path $projectRoot "limine-bios-x86_64.bin"

    Write-Host "Baixando $binUrl ..."
    Invoke-WebRequest -Uri $binUrl -OutFile $destBin -Headers @{'User-Agent' = 'PowerShell'}

    if (-not (Test-Path $destBin)) {
        Write-Error "Falha ao baixar o binário Limine."
        exit 1
    }
    $limineBin = $destBin
    Write-Host "Download concluído: $limineBin"
}

# ------------------------------------------------------------------
# 3) Definir caminhos de trabalho
# ------------------------------------------------------------------
$isoPath = Join-Path $projectRoot "jinn.iso"

# Remove ISO anterior
if (Test-Path $isoPath) { Remove-Item $isoPath -Force }

# ------------------------------------------------------------------
# 4) Localizar mkisofs / genisoimage / xorriso
# ------------------------------------------------------------------
$mkiso = $null
if (Get-Command mkisofs -ErrorAction SilentlyContinue) {
    $mkiso = "mkisofs"
} elseif (Get-Command genisoimage -ErrorAction SilentlyContinue) {
    $mkiso = "genisoimage"
} elseif (Get-Command xorriso -ErrorAction SilentlyContinue) {
    $mkiso = "xorriso"
} else {
    # Procura em locais comuns de instalação
    $knownPaths = @(
        "C:\Program Files\Git\usr\bin\mkisofs.exe",
        "C:\msys64\usr\bin\mkisofs.exe",
        "C:\msys64\mingw64\bin\mkisofs.exe",
        "C:\msys64\usr\bin\xorriso.exe",
        "C:\msys64\mingw64\bin\xorriso.exe",
        "C:\cygwin64\bin\mkisofs.exe"
    )
    foreach ($p in $knownPaths) {
        if (Test-Path $p) { $mkiso = $p; break }
    }
}

if (-not $mkiso) {
    # try Windows oscdimg as a fallback (part of Windows ADK)
    if (Get-Command oscdimg -ErrorAction SilentlyContinue) {
        $mkiso = "oscdimg"
        Write-Host "mkisofs/genisoimage não encontrado, usando oscdimg como fallback."
    } else {
        Write-Error @"
    'mkisofs', 'genisoimage' ou 'xorriso' não encontrado.
Instale um deles ou o Windows ADK (fornece 'oscdimg').
Opções:
  - Git Bash     : https://git-scm.com/download/win  (já inclui mkisofs)
    - MSYS2        : https://www.msys2.org  → pacman -S xorriso
  - Windows ADK  : https://learn.microsoft.com/en-us/windows-hardware/get-started/adk-install
Depois reinicie este PowerShell.
"@
        exit 1
    }
}
Write-Host "Usando ferramenta de criação de ISO: $mkiso"

# ------------------------------------------------------------------
# 5) Montar a estrutura de boot para a ISO
# ------------------------------------------------------------------
# O mkisofs precisa de um diretório com os arquivos a gravar.
# A estrutura deve corresponder aos caminhos usados pelo limine.conf.
$bootStage = Join-Path $projectRoot "iso_stage"
if (Test-Path $bootStage) { Remove-Item $bootStage -Recurse -Force }
New-Item -ItemType Directory -Path $bootStage | Out-Null
New-Item -ItemType Directory -Path (Join-Path $bootStage "boot") | Out-Null
if ($bootx64Efi) {
    New-Item -ItemType Directory -Path (Join-Path $bootStage "EFI\BOOT") -Force | Out-Null
}

# Binário Limine (para o El Torito)
if ($limineCdBin) {
    Copy-Item -Path $limineCdBin -Destination $bootStage -Force
} else {
    Copy-Item -Path $limineBin -Destination $bootStage -Force
}
Copy-Item -Path (Join-Path $projectRoot "iso_root\limine.conf") -Destination $bootStage -Force
if ($limineSys) {
    Copy-Item -Path $limineSys -Destination $bootStage -Force
}
if ($bootx64Efi) {
    Copy-Item -Path $bootx64Efi -Destination (Join-Path $bootStage "EFI\BOOT\BOOTX64.EFI") -Force
}

# Kernel compilado pela crate principal (o ELF gerado pelo build.ps1)
$kernelELF = Join-Path $projectRoot "target\x86_64-unknown-none\release\jinn"
if (-not (Test-Path $kernelELF)) {
    $kernelELF = Join-Path $projectRoot "boot\kernel.elf"
}
if (Test-Path $kernelELF) {
    Copy-Item -Path $kernelELF -Destination (Join-Path $bootStage "boot\kernel.elf") -Force
} else {
    Write-Warning "Kernel não encontrado em $kernelELF. A ISO pode não bootar o kernel."
}

# ------------------------------------------------------------------
# 6) Gerar a ISO com El Torito (BIOS boot via Limine)
# ------------------------------------------------------------------
$limineBinName = if ($limineCdBin) { Split-Path $limineCdBin -Leaf } else { Split-Path $limineBin -Leaf }
$mkisoArgs = @(
    "-o", $isoPath,
    "-b", $limineBinName,
    "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
    "-quiet",
    $bootStage
)
if ($bootx64Efi) {
    $mkisoArgs = @(
        "-o", $isoPath,
        "-b", $limineBinName,
        "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
        "-e", "EFI/BOOT/BOOTX64.EFI", "-no-emul-boot",
        "-quiet",
        $bootStage
    )
}

if ((Split-Path $mkiso -Leaf) -eq "xorriso.exe" -or $mkiso -eq "xorriso") {
    $isoPathUnix = (& "C:\msys64\usr\bin\cygpath.exe" -u $isoPath).Trim()
    $bootStageUnix = (& "C:\msys64\usr\bin\cygpath.exe" -u $bootStage).Trim()
    $env:MSYS_NO_PATHCONV = "1"
    $mkisoArgs = @(
        "-as", "mkisofs", "-o", $isoPathUnix,
        "-b", $limineBinName,
        "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
        "-quiet",
        $bootStageUnix
    )
    if ($bootx64Efi) {
        $mkisoArgs = @(
            "-as", "mkisofs", "-o", $isoPathUnix,
            "-b", $limineBinName,
            "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
            "-e", "EFI/BOOT/BOOTX64.EFI", "-no-emul-boot",
            "-quiet",
            $bootStageUnix
        )
    }
}

Write-Host "Gerando ISO em $isoPath ..."
& $mkiso @mkisoArgs

if (-not (Test-Path $isoPath)) {
    Write-Error "Geração da ISO falhou."
    exit 1
}
Write-Host "ISO gerada com sucesso."

# ------------------------------------------------------------------
# 7) Iniciar o QEMU
# ------------------------------------------------------------------
if (-not $NoRun) {
    Write-Host "Iniciando QEMU..."
    $qemu = (Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue).Source
    if (-not $qemu) {
        $qemuCandidates = @(
            "C:\Program Files\qemu\qemu-system-x86_64.exe",
            "C:\Program Files\QEMU\qemu-system-x86_64.exe"
        )
        $qemu = $qemuCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $qemu) {
        Write-Error "qemu-system-x86_64 não encontrado. Adicione o QEMU ao PATH."
        exit 1
    }
    & $qemu `
        -cdrom  $isoPath `
        -m      512M `
        -serial stdio `
        -display sdl
} else {
    Write-Host "ISO gerada; execução do QEMU ignorada por -NoRun."
}

# ------------------------------------------------------------------
# 8) Limpeza
# ------------------------------------------------------------------
Remove-Item $bootStage -Recurse -Force -ErrorAction SilentlyContinue
