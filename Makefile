# Makefile para Jinn OS
# Suporta ambientes Linux, macOS e WSL.

CARGO ?= cargo
RUST_TARGET ?= x86_64-unknown-none
QEMU ?= qemu-system-x86_64
XORRISO ?= xorriso

KERNEL_ELF = boot/kernel.elf
APP_ELF = apps/ola-mundo/target/x86_64-jinn-user/release/ola-mundo
ISO = jinn.iso
DISK_IMG = disk.img

LIMINE_BRANCH ?= v8.x-binary

.PHONY: all kernel app disk iso run clean help

all: iso disk

help:
	@echo "Comandos disponíveis:"
	@echo "  make kernel   - Compila o kernel do Jinn OS em modo release"
	@echo "  make app      - Compila a aplicação de usuário ola-mundo (Ring 3)"
	@echo "  make disk     - Gera o disco virtual FAT32 (disk.img)"
	@echo "  make iso      - Gera a imagem ISO híbrida inicializável (jinn.iso) com Limine"
	@echo "  make run      - Inicia o QEMU com a ISO e o disco virtual FAT32"
	@echo "  make clean    - Remove artefatos de compilação e imagens geradas"

kernel:
	@echo "[+] Compilando Jinn Kernel..."
	$(CARGO) build --release --target $(RUST_TARGET) -Zbuild-std=core,alloc
	@mkdir -p boot iso_root/boot
	@cp target/$(RUST_TARGET)/release/jinn $(KERNEL_ELF)
	@cp target/$(RUST_TARGET)/release/jinn iso_root/boot/kernel.elf

app:
	@echo "[+] Compilando aplicação de usuário Ring 3 (ola-mundo)..."
	@cd apps/ola-mundo && $(CARGO) +nightly build --release -Zjson-target-spec -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem
	@mkdir -p iso_root/apps
	@cp $(APP_ELF) iso_root/apps/ola-mundo.elf

limine:
	@if [ ! -d "limine" ]; then \
		echo "[+] Baixando componentes do Limine Bootloader..."; \
		git clone https://github.com/limine-bootloader/limine.git --branch=$(LIMINE_BRANCH) --depth=1; \
		$(MAKE) -C limine; \
	fi

disk: app
	@echo "[+] Verificando/Gerando imagem de disco FAT32 (disk.img)..."
	@if command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/make_disk.ps1; \
	elif command -v powershell >/dev/null 2>&1; then \
		powershell -NoProfile -ExecutionPolicy Bypass -File scripts/make_disk.ps1; \
	elif command -v python3 >/dev/null 2>&1; then \
		python3 scripts/make_disk.py; \
	elif command -v python >/dev/null 2>&1; then \
		python scripts/make_disk.py; \
	fi

iso: kernel app limine
	@echo "[+] Preparando estrutura iso_root..."
	@mkdir -p iso_root/boot iso_root/apps iso_root/EFI/BOOT
	@cp limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/ 2>/dev/null || true
	@cp limine/BOOTX64.EFI iso_root/EFI/BOOT/ 2>/dev/null || true
	@echo "[+] Gerando ISO híbrida com xorriso..."
	$(XORRISO) -as mkisofs -b boot/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(ISO)
	@echo "[+] Instalando Limine MBR na ISO..."
	./limine/limine bios-install $(ISO)
	@echo "[SUCCESS] $(ISO) gerada com sucesso!"

run: all
	@echo "[+] Iniciando QEMU com Jinn OS..."
	$(QEMU) -boot order=d -cdrom $(ISO) -drive file=$(DISK_IMG),format=raw,index=0,media=disk -m 512M -serial stdio

clean:
	@echo "[+] Limpando diretórios de build..."
	$(CARGO) clean
	@rm -rf boot/kernel.elf $(ISO) $(DISK_IMG)
