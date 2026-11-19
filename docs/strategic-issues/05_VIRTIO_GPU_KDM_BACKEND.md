# [KDM / Drivers] VirtIO-GPU Compute / Zero-Copy DMA Backend for JHCS

**Repository:** `GeovaneEstanislau/Jinn_Project`  
**Labels:** `help wanted`, `jhcs`, `drivers`, `dma`, `virtio`, `gpu`

---

## 📖 Architectural Context

The **Kernel DMA Manager (KDM)** in Jinn OS provides physical memory allocation, temporal 64-bit generation tokens, and bounce buffer isolation via Syscalls 60 through 68. 

Currently, graphical display uses standard Limine linear framebuffers. To enable true hardware-accelerated compute in virtualized and embedded environments, we want to implement a userspace driver and compute backend for **VirtIO-GPU** (`PCI ID 1AF4:1050`), allowing the JHCS in Ring 3 to allocate command queues via KDM and submit compute operations directly to the virtualized device without data copies through kernel space.

---

## 🎯 Scope of Work

1. Implement PCI capability discovery for VirtIO-GPU (`vendor_id = 0x1AF4`, `device_id = 0x1050`) in user space.
2. Interface with KDM syscalls:
   - `sys_dma_pool_create` / `sys_dma_buffer_alloc` (Syscalls 60 and 62) to allocate 4 KiB-aligned contiguous physical buffers for the VirtIO controlq and cursorq.
   - `sys_dma_transfer_prepare` (Syscall 66) to validate temporal DMA tokens.
3. Implement basic VirtIO-GPU command submission:
   - `VIRTIO_GPU_CMD_GET_DISPLAY_INFO`
   - `VIRTIO_GPU_CMD_RESOURCE_CREATE_2D`
   - `VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D`
4. Guarantee graceful degradation: if VirtIO-GPU is absent or uninitialized, automatically fall back to CPU SIMD.

---

## 📂 Files Involved

- `src/dma/manager.rs`
- `src/drivers/pci.rs`
- `crates/jhcs-platform-jinn/src/lib.rs`
- `apps/aladdin-server/src/` or `apps/jhcs-server/src/`

---

## 🧪 How to Test Locally

```powershell
# 1. Run QEMU with VirtIO-GPU attached
& "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe" `
  -cdrom jinn.iso `
  -device virtio-gpu-pci `
  -serial file:qemu_serial.log `
  -m 64M

# 2. Inspect initialization logs
Get-Content qemu_serial.log | Select-String -Pattern "virtio-gpu"
```

---

## ✅ Acceptance Criteria

- [ ] Userspace driver identifies VirtIO-GPU PCI BARs and initializes command rings using KDM memory tokens.
- [ ] Submits 2D resource create commands without kernel crashes.
- [ ] If device is absent, gracefully falls back to CPU SIMD without panics.
