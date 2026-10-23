/// Jinn OS — JDA Physical Frame Zero-Copy Buffer Management
///
/// Implementa a gestão de buffers de páginas físicas reais compartilhadas entre
/// tarefas e o microkernel, eliminando duplicações de memória (`memcpy`) via
/// remapeamento no Page Table Level 4 (PML4).

use crate::memory::{self, SpinLock};

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
        }
    }
}

pub struct SharedBufferPool {
    buffers: [PhysicalPageBuffer; MAX_SHARED_BUFFERS],
    generations: [u16; MAX_SHARED_BUFFERS],
}

impl SharedBufferPool {
    pub const fn new() -> Self {
        Self {
            buffers: [PhysicalPageBuffer::empty(); MAX_SHARED_BUFFERS],
            generations: [1; MAX_SHARED_BUFFERS],
        }
    }

    /// Valida e decodifica um handle (geração de 16 bits + índice de 16 bits)
    fn decode_handle(&self, handle: usize) -> Option<usize> {
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
                self.buffers[i] = PhysicalPageBuffer {
                    id: i,
                    frame_phys_addr: frame as u64,
                    page_count: 1,
                    ref_count: 1,
                    is_read_only,
                    owner_pid,
                    is_allocated: true,
                };
                let handle = ((gen as usize) << 16) | (i & 0xFFFF);
                return Ok(handle);
            }
        }

        memory::free_frame(frame);
        Err("Tabela de buffers compartilhados do JDA cheia")
    }

    /// Incrementa a contagem de referências ao vincular o buffer a um novo nó ou processo
    pub fn retain(&mut self, handle: usize) -> Result<u64, &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        self.buffers[slot].ref_count += 1;
        Ok(self.buffers[slot].frame_phys_addr)
    }

    /// Decrementa a contagem de referências. Libera o frame físico se atingir 0
    pub fn release(&mut self, caller_pid: usize, handle: usize) -> Result<(), &'static str> {
        let slot = self.decode_handle(handle).ok_or("Descritor de buffer compartilhado invalido ou obsoleto")?;
        if caller_pid != 0 && self.buffers[slot].owner_pid != caller_pid {
            return Err("Permissao negada: buffer pertence a outro processo");
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

    /// Libera todos os buffers físicos alocados pertencentes a `pid`.
    /// Chamado automaticamente pelo escalonador no encerramento da tarefa (`exit_task`).
    pub fn release_all_for_pid(&mut self, pid: usize) {
        for i in 0..MAX_SHARED_BUFFERS {
            if self.buffers[i].is_allocated && self.buffers[i].owner_pid == pid {
                memory::free_frame(self.buffers[i].frame_phys_addr);
                self.buffers[i] = PhysicalPageBuffer::empty();
                self.generations[i] = self.generations[i].wrapping_add(1);
                if self.generations[i] == 0 {
                    self.generations[i] = 1;
                }
            }
        }
    }

    /// Obtém o endereço físico do buffer validando a propriedade de `caller_pid`
    pub fn get_phys_addr(&self, caller_pid: usize, handle: usize) -> Option<u64> {
        let slot = self.decode_handle(handle)?;
        if caller_pid == 0 || self.buffers[slot].owner_pid == caller_pid {
            Some(self.buffers[slot].frame_phys_addr)
        } else {
            None
        }
    }

    /// Lê dados de um dispositivo de blocos diretamente para o frame físico compartilhado (Zero-Copy Block I/O).
    /// Elimina qualquer cópia intermediária no heap do kernel ou buffers temporários.
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

        let virt_addr = crate::memory::paging::phys_to_virt(phys);
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

        let virt_addr = crate::memory::paging::phys_to_virt(phys);
        let slice = unsafe { core::slice::from_raw_parts(virt_addr as *const u8, total_bytes) };
        dev.write_blocks(start_block, count, slice)
    }
}

pub static SHARED_BUFFER_POOL: SpinLock<SharedBufferPool> = SpinLock::new(SharedBufferPool::new());
