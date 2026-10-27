/// Jinn OS — JDA Physical Frame Zero-Copy Buffer Management
///
/// Implementa a gestão de buffers de páginas físicas reais compartilhadas entre
/// tarefas e o microkernel, eliminando duplicações de memória (`memcpy`) via
/// remapeamento no Page Table Level 4 (PML4).

use crate::memory::{self, SpinLock};
use crate::scheduler::{self, USER_JDA_BASE};
use crate::memory::paging::{self, PageTable, USER_RO, USER_RW};

pub const MAX_SHARED_BUFFERS: usize = 32;

/// Descritor de um conjunto de páginas físicas compartilhadas sem cópia
#[derive(Debug, Clone, Copy)]
pub struct PhysicalPageBuffer {
    pub id: usize,
    pub frame_phys_addr: u64,
    pub page_count: usize,
    pub ref_count: usize,
    pub is_read_only: bool,
    pub owner_pid: usize,
    pub is_allocated: bool,
    pub mapped_mask: u32,
}

impl PhysicalPageBuffer {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            frame_phys_addr: 0,
            page_count: 0,
            ref_count: 0,
            is_read_only: false,
            owner_pid: 0,
            is_allocated: false,
            mapped_mask: 0,
        }
    }
}

pub struct SharedBufferPool {
    pub buffers: [PhysicalPageBuffer; MAX_SHARED_BUFFERS],
    pub generations: [u16; MAX_SHARED_BUFFERS],
}

impl SharedBufferPool {
    pub const fn new() -> Self {
        Self {
            buffers: [PhysicalPageBuffer::empty(); MAX_SHARED_BUFFERS],
            generations: [1; MAX_SHARED_BUFFERS],
        }
    }

    /// Valida e decodifica um handle (geração de 16 bits + índice de 16 bits)
    pub fn decode_handle(&self, handle: usize) -> Option<usize> {
        let slot = handle & 0xFFFF;
        let gen = ((handle >> 16) & 0xFFFF) as u16;
        if slot < MAX_SHARED_BUFFERS && self.buffers[slot].is_allocated && self.generations[slot] == gen {
            Some(slot)
        } else {
            None
        }
    }

    /// Aloca um novo buffer de página física para transferência Zero-Copy
    pub fn allocate(&mut self, owner_pid: usize, is_read_only: bool) -> Result<usize, &'static str> {
        let frame = memory::alloc_frame().ok_or("Sem quadros fisicos de memoria disponiveis")?;

        for i in 0..MAX_SHARED_BUFFERS {
            if !self.buffers[i].is_allocated {
                let gen = self.generations[i];

                // Limpa o frame físico alocado
                let virt_kern = paging::phys_to_virt(frame as u64) as *mut u8;
                unsafe { core::ptr::write_bytes(virt_kern, 0, 4096); }

                let mut mapped_mask = 0u32;

                // Mapeia diretamente no PML4 do processo de usuário se owner_pid > 0
                if owner_pid > 0 {
                    let sched = scheduler::get();
                    if let Some(Some(ref task)) = sched.tasks.get(owner_pid) {
                        let pml4 = paging::phys_to_virt(task.pml4_phys) as *mut PageTable;
                        let vaddr = USER_JDA_BASE + (i as u64 * 4096);
                        let flags = if is_read_only { USER_RO } else { USER_RW };
                        unsafe {
                            paging::map_page(pml4, vaddr, frame as u64, flags);
                        }
                        mapped_mask |= 1 << (owner_pid as u32);
                    }
                }

                self.buffers[i] = PhysicalPageBuffer {
                    id: i,
                    frame_phys_addr: frame as u64,
                    page_count: 1,
                    ref_count: 1,
                    is_read_only,
                    owner_pid,
                    is_allocated: true,
                    mapped_mask,
                };
                let handle = ((gen as usize) << 16) | (i & 0xFFFF);
                return Ok(handle);
            }
        }

        memory::free_frame(frame);
        Err("Tabela de buffers compartilhados do JDA cheia")
    }

    /// Retorna o endereço virtual mapeado no espaço de usuário para o buffer
    pub fn get_user_vaddr(&self, caller_pid: usize, handle: usize) -> Option<u64> {
        let slot = self.decode_handle(handle)?;
        if caller_pid == 0 || (self.buffers[slot].mapped_mask & (1 << (caller_pid as u32))) != 0 {
            Some(USER_JDA_BASE + (slot as u64 * 4096))
        } else {
            None
        }
    }

    /// Compartilha um buffer com outro processo de usuário (`target_pid`) em modo somente-leitura (USER_RO)
    pub fn share(
        &mut self,
        caller_pid: usize,
        handle: usize,
        target_pid: usize,
        make_owner_ro: bool,
    ) -> Result<u64, &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido")?;
        if caller_pid != 0 && self.buffers[slot].owner_pid != caller_pid {
            return Err("Permissao negada: buffer pertence a outro processo");
        }
        if target_pid >= scheduler::MAX_TASKS {
            return Err("PID alvo invalido");
        }

        let phys = self.buffers[slot].frame_phys_addr;
        let vaddr = USER_JDA_BASE + (slot as u64 * 4096);

        // Mapeia no PML4 do processo alvo como USER_RO
        let sched = scheduler::get();
        if let Some(Some(ref target_task)) = sched.tasks.get(target_pid) {
            let target_pml4 = paging::phys_to_virt(target_task.pml4_phys) as *mut PageTable;
            unsafe {
                paging::map_page(target_pml4, vaddr, phys, USER_RO);
            }
            self.buffers[slot].ref_count += 1;
            self.buffers[slot].mapped_mask |= 1 << (target_pid as u32);
        } else {
            return Err("Processo alvo nao encontrado");
        }

        // Se solicitado, rebaixa também a página do processo proprietário para USER_RO (garantia formal de imutabilidade)
        if make_owner_ro && caller_pid > 0 {
            if let Some(Some(ref caller_task)) = sched.tasks.get(caller_pid) {
                let caller_pml4 = paging::phys_to_virt(caller_task.pml4_phys) as *mut PageTable;
                unsafe {
                    let _ = paging::set_page_flags(caller_pml4, vaddr, USER_RO);
                }
                self.buffers[slot].is_read_only = true;
            }
        }

        // Registra telemetria de 4 KiB poupados via Zero-Copy
        crate::jda::telemetry::record_data_transfer(4096, true);

        Ok(vaddr)
    }

    /// Incrementa a contagem de referências ao vincular o buffer a um novo nó ou processo
    pub fn retain(&mut self, handle: usize) -> Result<u64, &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        self.buffers[slot].ref_count += 1;
        Ok(self.buffers[slot].frame_phys_addr)
    }

    /// Decrementa a contagem de referências. Desmapeia do PML4 do chamador e libera o frame físico se atingir 0
    pub fn release(&mut self, caller_pid: usize, handle: usize) -> Result<(), &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        if caller_pid != 0 && self.buffers[slot].owner_pid != caller_pid && (self.buffers[slot].mapped_mask & (1 << (caller_pid as u32))) == 0 {
            return Err("Permissao negada: buffer pertence a outro processo");
        }

        // Desmapeia do PML4 do chamador se estiver mapeado
        if caller_pid > 0 && (self.buffers[slot].mapped_mask & (1 << (caller_pid as u32))) != 0 {
            let sched = scheduler::get();
            if let Some(Some(ref task)) = sched.tasks.get(caller_pid) {
                let pml4 = paging::phys_to_virt(task.pml4_phys) as *mut PageTable;
                let vaddr = USER_JDA_BASE + (slot as u64 * 4096);
                unsafe {
                    paging::unmap_page(pml4, vaddr);
                }
            }
            self.buffers[slot].mapped_mask &= !(1 << (caller_pid as u32));
        }

        if self.buffers[slot].ref_count > 1 {
            self.buffers[slot].ref_count -= 1;
        } else {
            // Última referência: devolve o frame físico ao alocador de memória
            memory::free_frame(self.buffers[slot].frame_phys_addr);
            self.buffers[slot] = PhysicalPageBuffer::empty();
            self.generations[slot] = self.generations[slot].wrapping_add(1);
            if self.generations[slot] == 0 {
                self.generations[slot] = 1;
            }
        }

        Ok(())
    }

    /// Libera todos os buffers físicos alocados pertencentes a `pid` ou compartilhados com ele.
    /// Chamado automaticamente pelo escalonador no encerramento da tarefa (`exit_task`).
    pub fn release_all_for_pid(&mut self, pid: usize) {
        if pid == 0 {
            return;
        }
        let pid_bit = 1 << (pid as u32);
        for i in 0..MAX_SHARED_BUFFERS {
            if self.buffers[i].is_allocated && ((self.buffers[i].mapped_mask & pid_bit) != 0 || self.buffers[i].owner_pid == pid) {
                // Desmapeia do PML4 do processo
                let sched = scheduler::get();
                if let Some(Some(ref task)) = sched.tasks.get(pid) {
                    let pml4 = paging::phys_to_virt(task.pml4_phys) as *mut PageTable;
                    let vaddr = USER_JDA_BASE + (i as u64 * 4096);
                    unsafe {
                        paging::unmap_page(pml4, vaddr);
                    }
                }
                self.buffers[i].mapped_mask &= !pid_bit;

                if self.buffers[i].ref_count > 1 {
                    self.buffers[i].ref_count -= 1;
                } else {
                    memory::free_frame(self.buffers[i].frame_phys_addr);
                    self.buffers[i] = PhysicalPageBuffer::empty();
                    self.generations[i] = self.generations[i].wrapping_add(1);
                    if self.generations[i] == 0 {
                        self.generations[i] = 1;
                    }
                }
            }
        }
    }

    /// Consulta se o buffer é somente-leitura
    pub fn is_read_only(&self, handle: usize) -> bool {
        match self.decode_handle(handle) {
            Some(slot) => self.buffers[slot].is_read_only,
            None => true,
        }
    }

    /// Obtém o endereço físico do buffer validando a propriedade ou compartilhamento de `caller_pid`
    pub fn get_phys_addr(&self, caller_pid: usize, handle: usize) -> Option<u64> {
        let slot = self.decode_handle(handle)?;
        if caller_pid == 0 || self.buffers[slot].owner_pid == caller_pid || (self.buffers[slot].mapped_mask & (1 << (caller_pid as u32))) != 0 {
            Some(self.buffers[slot].frame_phys_addr)
        } else {
            None
        }
    }

    /// Lê dados de um dispositivo de blocos diretamente para o frame físico compartilhado (Zero-Copy Block I/O).
    pub fn read_from_block_device<D: crate::drivers::block::BlockDevice>(
        &self,
        handle: usize,
        dev: &D,
        start_block: u64,
        count: usize,
    ) -> Result<(), &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        let phys = self.buffers[slot].frame_phys_addr;
        let total_bytes = count * dev.block_size();
        if total_bytes > 4096 {
            return Err("Quantidade de blocos excede a capacidade do frame de 4 KiB");
        }

        let virt_addr = paging::phys_to_virt(phys);
        let slice = unsafe { core::slice::from_raw_parts_mut(virt_addr as *mut u8, total_bytes) };
        dev.read_blocks(start_block, count, slice)
    }

    /// Escreve dados de um frame físico compartilhado diretamente em um dispositivo de blocos (Zero-Copy Direct Flush).
    pub fn write_to_block_device<D: crate::drivers::block::BlockDevice>(
        &self,
        handle: usize,
        dev: &D,
        start_block: u64,
        count: usize,
    ) -> Result<(), &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        let phys = self.buffers[slot].frame_phys_addr;
        let total_bytes = count * dev.block_size();
        if total_bytes > 4096 {
            return Err("Quantidade de blocos excede a capacidade do frame de 4 KiB");
        }

        let virt_addr = paging::phys_to_virt(phys);
        let slice = unsafe { core::slice::from_raw_parts(virt_addr as *const u8, total_bytes) };
        dev.write_blocks(start_block, count, slice)
    }
}

pub static SHARED_BUFFER_POOL: SpinLock<SharedBufferPool> = SpinLock::new(SharedBufferPool::new());
