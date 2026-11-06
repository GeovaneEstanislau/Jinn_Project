# Jinn OS - Matriz Automatizada de Validacao de Hardware (JAHA 1.0)
param (
    [string]$TargetProfile = "all"
)

$qemuPath = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemuPath)) {
    Write-Error "QEMU nao encontrado em: $qemuPath"
    exit 1
}

$matrix = @(
    @{ Name = "Legacy";      Ram = "256M";  Cpu = 1; Type = "Oficial" },
    @{ Name = "Compact";     Ram = "512M";  Cpu = 1; Type = "Oficial" },
    @{ Name = "Standard";    Ram = "1024M"; Cpu = 2; Type = "Oficial" },
    @{ Name = "Performance"; Ram = "4096M"; Cpu = 4; Type = "Oficial" },
    @{ Name = "Stress-128M"; Ram = "128M";  Cpu = 1; Type = "Experimental" },
    @{ Name = "Stress-64M";  Ram = "64M";   Cpu = 1; Type = "Experimental" }
)

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host " JINN OS - EXECUCAO DA MATRIZ MULTI-PERFIL DE HARDWARE (JAHA 1.0)" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

foreach ($tier in $matrix) {
    if ($TargetProfile -ne "all" -and $tier.Name -ne $TargetProfile) {
        continue
    }

    $logFile = "qemu_matrix_$($tier.Name).log"
    if (Test-Path $logFile) {
        Remove-Item $logFile -Force
    }

    Write-Host "`n[+] Testando Perfil $($tier.Name) (RAM: $($tier.Ram), vCPUs: $($tier.Cpu), Status: $($tier.Type))..." -ForegroundColor Yellow

    $qArgs = "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m $($tier.Ram) -smp $($tier.Cpu) -display none -serial file:$logFile"
    $p = Start-Process -FilePath $qemuPath -ArgumentList $qArgs -PassThru
    Start-Sleep -Seconds 10
    Stop-Process -Id $p.Id -Force
    Start-Sleep -Milliseconds 500

    if (Test-Path $logFile) {
        $content = Get-Content $logFile -Raw
        $bootOk = $content.Contains("Jinn Microkernel")
        $profilerOk = $content.Contains("Hardware Profiler")
        $driverOk = $content.Contains("Driver Supervisor")
        $schedOk = $content.Contains("controle ao Escalonador") -or $content.Contains("Processo Ring 3 criado")

        if ($bootOk -and $profilerOk -and $driverOk -and $schedOk) {
            Write-Host "    [V] APROVADO: Boot, Profiler, Driver Supervisor e Escalonador operacionais!" -ForegroundColor Green
            $lines = $content -split "`r?`n"
            $perfLine = $lines | Where-Object { $_ -match "Perfil Ativo" } | Select-Object -First 1
            $memLine  = $lines | Where-Object { $_ -match "Memoria Utilizavel" } | Select-Object -First 1
            if ($perfLine) { Write-Host "    $perfLine" -ForegroundColor Gray }
            if ($memLine)  { Write-Host "    $memLine" -ForegroundColor Gray }
        } else {
            Write-Host "    [X] FALHA OU RESTRICAO no Perfil $($tier.Name)!" -ForegroundColor Red
            if (-not $bootOk)     { Write-Host "        - Falha no boot do kernel" -ForegroundColor DarkRed }
            if (-not $profilerOk) { Write-Host "        - Falha na checagem do Hardware Profiler" -ForegroundColor DarkRed }
            if (-not $driverOk)   { Write-Host "        - Falha nos testes de isolamento de drivers" -ForegroundColor DarkRed }
            if (-not $schedOk)    { Write-Host "        - Kernel nao alcancou o escalonador" -ForegroundColor DarkRed }
        }
    } else {
        Write-Host "    [X] Log de execucao nao gerado!" -ForegroundColor Red
    }
}

Write-Host "`n================================================================================" -ForegroundColor Cyan
Write-Host " MATRIZ DE HARDWARE CONCLUIDA" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan
