# Jinn OS

Jinn is an experimental operating system written in Rust for `x86_64`, designed as a minimal kernel with a focus on simple boot, scheduler, timer, and memory management.

## Project goal

- `#![no_std]` kernel written in Rust
- Boot via Limine with VGA text output
- Modular structure for memory, timer, scheduler, PIC/IDT, and boot
- Technical documentation to enable review and further development

## Current status

- Prototype kernel with cooperative scheduler and example tasks
- Boot infrastructure and ISO generation scripts available
- Drivers, services, and full hardware support are still under development
- Repository is prepared for review, but this is not a production-ready operating system

## Repository structure

- `src/` - main kernel source code
- `boot/`, `iso_root/`, `limine/` - boot infrastructure and configuration
- `kernel/` - auxiliary or legacy crate
- `docs/` - technical documentation and project vision
- `scripts/` - build, ISO creation, and execution scripts
- `Cargo.toml`, `rust-toolchain.toml` - Rust build configuration
- `linker.ld`, `target.json` - linker and target configuration

## How to build

1. Install Rust and `nightly`:
   ```powershell
   rustup toolchain install nightly
   ```
2. Add the bare-metal target:
   ```powershell
   rustup target add x86_64-unknown-none
   ```
3. Build the kernel:
   ```powershell
   cargo build --release --target x86_64-unknown-none
   ```

## How to run

- Use `scripts/run.ps1` to generate the ISO and run it in QEMU.
- The script can download Limine binaries automatically if they are not present.
- The file `iso_root/boot/jinn_kernel/jinn` is generated during build/ISO creation and should not be included in the repository.

## Generated ISO

- `jinn.iso` is available at the repository root.
- SHA256: `80A99E4E54C6146222FE21007FA629B707E1337A26863AF98BADCC66310B371D`

## Documentation

- `docs/README.md` — index of technical documents
- `docs/PROJECT_OVERVIEW.md` — project overview and goals
- `docs/architecture/` — architectural vision of Jinn OS
- `docs/kernel/` — kernel and scheduler design
- `docs/services/` — planned user-space services

## Contributing

- See `CONTRIBUTING.md` for contribution guidelines
- Open issues for bugs and feature proposals
- Keep commits small and descriptive
- Run `cargo build --release --target x86_64-unknown-none` before submitting changes

## License

- See `LICENSE` for the project terms

## Notes

- Build artifacts and logs are filtered by `.gitignore`
- The `limine/` folder contains third-party bootloader artifacts and does not need to be included in the main repository
