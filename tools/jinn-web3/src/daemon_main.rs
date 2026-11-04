//! Jinn Web3 Daemon (`jinn-web3d`) Service Entrypoint
//!
//! Autonomous background daemon running in Ring 3 userspace on Jinn OS.
//! Provides IPC services for Aladdin Desktop (Wallet, Store) to interact
//! with BNB Smart Chain and the $JINN smart contracts.

mod bep20;
mod daemon;
mod invariants;
mod keccak;
mod keystore;
mod merkle;
mod rpc;

use daemon::*;

fn main() {
    println!("========================================================");
    println!("        jinn-web3d — Web3 Userspace Daemon (Ring 3)     ");
    println!("========================================================");
    println!("Inicializando servico jinn-web3d no Jinn OS Userspace...");

    let config = DaemonConfig::default();
    println!("Configuracao Carregada:");
    println!("  Rede Ativa:      BSC Chapel Testnet (Chain ID {})", config.chain_id);
    println!("  Endpoint RPC:    {}", config.rpc_endpoint);
    println!("  Token $JINN:     {}", bep20::to_hex_string(&config.token_contract));
    println!("  PoUC Bounties:   {}", bep20::to_hex_string(&config.distributor_contract));
    println!("  Gas Maximo:      {} unidades", config.max_gas_limit);

    let mut daemon = Web3Daemon::new(config);
    println!("\nValidando isolamento de privilegios...");
    assert!(daemon.verify_ring3_isolation());
    println!("  -> Isolamento Ring 3 / Ring 0 verificado com sucesso.");
    println!("  -> Nenhuma chave privada ou segredo reside no espaco de kernel.");

    println!("\nServico jinn-web3d ativo e aguardando mensagens IPC via Ring 3.");
    println!("Pressione Ctrl+C para encerrar o daemon.");

    // Simulação do loop de despacho IPC
    println!("\n--- Simulacao de Ciclo de Vida IPC ---");
    let test_pid = 2048;

    // 1. GetStatus
    println!("[IPC IN] PID {}: Opcode GetStatus", test_pid);
    let (code, resp) = daemon.handle_ipc_request(test_pid, Web3IpcOpcode::GetStatus as u32, &[]);
    println!("[IPC OUT] Status: {}, Bytes: {}", code, resp.len());

    // 2. PrepareTransfer
    let recipient = [0x55; 20];
    let amount_wei = bep20::u128_to_u256_be(250 * 1_000_000_000_000_000_000u128); // 250 $JINN
    let mut payload = Vec::new();
    payload.extend_from_slice(&recipient);
    payload.extend_from_slice(&amount_wei);

    println!("\n[IPC IN] PID {}: Opcode PrepareTransfer (250 $JINN)", test_pid);
    let (code, resp) = daemon.handle_ipc_request(test_pid, Web3IpcOpcode::PrepareTransfer as u32, &payload);
    let tx_id = u64::from_be_bytes(resp[..8].try_into().unwrap());
    println!("[IPC OUT] Status: {}, TxID Criado: {}, Calldata Len: {}", code, tx_id, resp.len() - 8);

    // 3. SignAndSend
    let fake_signed_tx = b"signed_tx_envelope_from_aladdin_ring3_wallet";
    println!("\n[IPC IN] PID {}: Opcode SignAndSend", test_pid);
    let (code, resp) = daemon.handle_ipc_request(test_pid, Web3IpcOpcode::SignAndSend as u32, fake_signed_tx);
    let mut tx_hash_bytes = [0u8; 32];
    tx_hash_bytes.copy_from_slice(&resp);
    println!("[IPC OUT] Status: {}, TxHash Transmitido: {}", code, bep20::to_hex_string(&tx_hash_bytes));

    println!("\nHistorico de Transacoes do Daemon: {} registro(s)", daemon.tx_history.len());
    println!("Logs de Auditoria Sem Segredos:     {} evento(s)", daemon.event_logs.len());
    println!("========================================================");
    println!("Daemon jinn-web3d finalizado com sucesso.");
}
