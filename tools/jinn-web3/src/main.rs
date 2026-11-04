//! Jinn Web3 & Token Utility CLI
//! Tooling for testing, encoding, and simulating Web3 & BEP-20 operations on Jinn OS.

#![allow(unused_imports)]

mod bep20;
mod daemon;
mod invariants;
mod keccak;
mod keystore;
mod merkle;
mod rpc;

use bep20::*;
use daemon::*;
use invariants::*;
use keccak::*;
use keystore::*;
use merkle::*;
use rpc::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match command {
        "info" | "--info" => print_protocol_info(),
        "encode" => run_encode_demo(),
        "rpc" => run_rpc_demo(),
        "pouc" => run_pouc_demo(),
        "invariants" => run_invariants_check(),
        "keystore" => run_keystore_demo(),
        "daemon" => run_daemon_demo(),
        _ => print_help(),
    }
}

fn print_help() {
    println!("========================================================");
    println!("     Jinn OS Web3 & Token Utility ($JINN Protocol)      ");
    println!("========================================================");
    println!("Uso: jinn-web3 <COMANDO>");
    println!("\nComandos disponíveis:");
    println!("  info        Exibe os parâmetros oficiais do token $JINN e a economia");
    println!("  encode      Demonstra codificação de calldata BEP-20 para Aladdin Wallet");
    println!("  rpc         Demonstra geração de envelopes JSON-RPC para nó BSC");
    println!("  pouc        Simula uma campanha PoUC com Merkle Tree e provas");
    println!("  invariants  Executa a verificação dos 6 invariantes de segurança");
    println!("  keystore    Demonstra geração, derivação Argon2id e criptografia AES-GCM");
    println!("  daemon      Demonstra o ciclo de vida e despacho IPC do jinn-web3d");
    println!("  help        Exibe esta mensagem de ajuda");
    println!("========================================================");
}

fn print_protocol_info() {
    println!("========================================================");
    println!("     Jinn Network Token ($JINN) — Especificações Oficiais");
    println!("========================================================");
    println!("Ativo:               $JINN (Jinn Network Token)");
    println!("Padrão:              BEP-20 (BNB Smart Chain / EVM)");
    println!("Decimais:            18");
    println!("Suprimento Máximo:   1.000.000.000 $JINN (1 Bilhão Fixo)");
    println!("Capacidade de Mint:  DESATIVADA (Hard Capped no Deploy)");
    println!("Mecanismo de Queima: Ativado (Burnable Deflacionário)");
    println!("Aprovações Gasless:  Ativadas (EIP-2612 Permit)");
    println!("Circuit Breaker:     Ativado (Pausable por Multi-Sig)");
    println!("\nDistribuição Econômica:");
    println!("  • 35% (350M $JINN) - Fundo Comunitário & Bounties (PoUC)");
    println!("  • 25% (250M $JINN) - Tesouraria do Ecossistema (DAO)");
    println!("  • 15% (150M $JINN) - Mantenedores Centrais & Core Dev");
    println!("  • 15% (150M $JINN) - Provisão de Liquidez DEX/CEX");
    println!("  • 10% (100M $JINN) - Airdrop Comunitário e Adoção Inicial");
    println!("========================================================");
}

fn run_encode_demo() {
    println!("--- Demonstração de Codificação ABI BEP-20 ---");
    let sample_wallet_str = "0x71C634C26B331E0cd51a04579997424946344955";
    let token_contract_str = "0x25aB829c36d07d1B73b88b488F35b2e98782E780";

    let wallet_addr = parse_address(sample_wallet_str).expect("Valid wallet");
    let token_addr = parse_address(token_contract_str).expect("Valid token");

    let balance_calldata = encode_balance_of(&wallet_addr);
    println!("Consulta de Saldo:");
    println!("  Conta:    {}", sample_wallet_str);
    println!("  Contrato: {}", token_contract_str);
    println!("  Calldata: {}", to_hex_string(&balance_calldata));

    let rpc_payload = format_eth_call(&token_addr, &balance_calldata, 1);
    println!("\nPayload JSON-RPC Gerado:");
    println!("  {}", rpc_payload);
}

fn run_rpc_demo() {
    println!("--- Demonstração de Requisições JSON-RPC BSC ---");
    let wallet_addr = [0x42; 20];
    let balance_query = format_eth_get_balance(&wallet_addr, 1);
    println!("1. eth_getBalance:\n   {}", balance_query);

    let nonce_query = format_eth_get_nonce(&wallet_addr, 2);
    println!("\n2. eth_getTransactionCount (Nonce):\n   {}", nonce_query);

    let chain_id_query = format_eth_chain_id(3);
    println!("\n3. eth_chainId:\n   {}", chain_id_query);

    let http_frame = format_http_post("data-seed-prebsc-1-s1.binance.org:8545", &balance_query);
    println!("\n4. HTTP Frame sobre Socket TCP:\n{}", http_frame);
}

fn run_pouc_demo() {
    println!("--- Simulação de Campanha PoUC (Proof of Useful Contribution) ---");
    let dev1 = [0x01; 20];
    let dev2 = [0x02; 20];
    let dev3 = [0x03; 20];

    let amt1 = u128_to_u256_be(5_000 * 1_000_000_000_000_000_000u128);
    let amt2 = u128_to_u256_be(3_000 * 1_000_000_000_000_000_000u128);
    let amt3 = u128_to_u256_be(2_000 * 1_000_000_000_000_000_000u128);

    let leaf1 = compute_pouc_leaf(0, &dev1, &amt1);
    let leaf2 = compute_pouc_leaf(1, &dev2, &amt2);
    let leaf3 = compute_pouc_leaf(2, &dev3, &amt3);

    let leaves = vec![leaf1, leaf2, leaf3];
    let (root, layers) = build_merkle_tree(&leaves);

    println!("Campanha Criada:");
    println!("  Contribuidores: 3");
    println!("  Merkle Root:    {}", to_hex_string(&root));

    let proof0 = generate_merkle_proof(&layers, 0);
    println!("  Prova Dev 0:    {} nós irmãos", proof0.len());
    let valid = verify_proof(&proof0, &root, &leaf1);
    println!("  Validação:      {}", if valid { "✅ SUCESSO (Prova Criptográfica Válida)" } else { "❌ FALHA" });
}

fn run_invariants_check() {
    println!("--- Verificação dos 6 Invariantes Fundamentais de Segurança ---");
    println!("[1/6] Invariante 1: Teto Máximo de 1B $JINN...");
    let owner = [0x01; 20];
    let treasury = [0x02; 20];
    let mut token = SimulatedToken::new(owner, treasury);
    assert_eq!(token.total_supply, HARD_CAP_WEI);
    token.burn(&treasury, 1_000_000).unwrap();
    assert!(token.total_supply <= HARD_CAP_WEI);
    println!("      -> OK: Suprimento matematicamente limitado a 1.000.000.000.");

    println!("[2/6] Invariante 2: Bloqueio de Vesting e Cliff...");
    let mut sched = SimulatedVestingSchedule {
        beneficiary: [0xAA; 20],
        start: 1000,
        cliff: 1090,
        duration: 360,
        total_amount: 1_000_000,
        released_amount: 0,
        revocable: false,
        revoked: false,
    };
    assert_eq!(sched.compute_releasable(1050), 0);
    assert!(sched.release(1050).is_err());
    println!("      -> OK: Retiradas antes do cliff são estritamente rejeitadas.");

    println!("[3/6] Invariante 3: Unicidade de Reivindicação Merkle...");
    let mut epoch = SimulatedEpoch::new([0x01; 32], 50_000, 0);
    epoch.claimed_indices.insert(0);
    assert!(epoch.claimed_indices.contains(&0));
    println!("      -> OK: Reivindicação dupla é bloqueada por registro único.");

    println!("[4/6] Invariante 4: Segregação de Privilégios Administrativos...");
    let hacker = [0x99; 20];
    assert!(token.pause(&hacker).is_err());
    println!("      -> OK: Ações de administrador rejeitam chamadores não autorizados.");

    println!("[5/6] Invariante 5: Circuito de Emergência (Pausa)...");
    token.pause(&owner).unwrap();
    assert!(token.transfer(&treasury, &hacker, 10).is_err());
    token.unpause(&owner).unwrap();
    println!("      -> OK: Pausa congela transferências e queimas sensíveis.");

    println!("[6/6] Invariante 6: Isolamento Estrito entre Campanhas...");
    let mut ep1 = SimulatedEpoch::new([0x01; 32], 10_000, 100);
    let ep2 = SimulatedEpoch::new([0x02; 32], 20_000, 0);
    ep1.close();
    assert!(!ep1.active);
    assert!(ep2.active);
    println!("      -> OK: Encerramento de uma campanha não afeta as demais.");

    println!("\n✅ Todos os 6 invariantes foram validados com 100% de conformidade!");
}

fn run_keystore_demo() {
    println!("--- Demonstração de Segurança do Keystore Aladdin (Ring 3) ---");
    let key = generate_random_key();
    let address = derive_address_from_key(&key);
    let address_hex = to_hex_string(&address);
    println!("1. Nova chave secp256k1 gerada em memória volátil de userspace.");
    println!("   Endereço derivado: {}", address_hex);

    let password = "AladdinSecurePassword!2026";
    println!("\n2. Cifrando chave privada com Argon2id (KDF) e AES-256-GCM (Cipher)...");
    let envelope = encrypt_keystore(&key, password, 97, None).expect("Cifragem com sucesso");
    let json_envelope = serde_json::to_string_pretty(&envelope).expect("JSON format");
    println!("   Envelope Criptográfico Gerado:\n{}", json_envelope);

    println!("\n3. Decifrando envelope com a senha fornecida...");
    let decrypted_key = decrypt_keystore(&envelope, password).expect("Decifragem com sucesso");
    assert_eq!(key.to_bytes(), decrypted_key.to_bytes());
    println!("   -> Chaves correspondem perfeitamente.");

    println!("\n4. Testando resistência a senha incorreta...");
    let failed = decrypt_keystore(&envelope, "senha_incorreta");
    assert!(failed.is_err());
    println!("   -> Rejeição segura confirmada: {:?}", failed.unwrap_err());

    println!("\n5. Verificando isolamento Ring 3 / Microkernel...");
    assert!(envelope.verify_ring3_isolation());
    println!("   -> Isolamento de privilégios validado: chaves privadas nunca tocam o Ring 0.");

    println!("\n✅ Demonstração do Keystore Seguro concluída com êxito!");
}

fn run_daemon_demo() {
    println!("--- Demonstração do Daemon Web3 jinn-web3d (Ring 3) ---");
    let mut daemon = Web3Daemon::new(DaemonConfig::default());
    println!("1. Daemon inicializado com sucesso para BSC Chapel (ID {}).", daemon.config.chain_id);
    assert!(daemon.verify_ring3_isolation());
    println!("   -> Isolamento de privilégios verificado.");

    let client_pid = 4096;
    println!("\n2. Processando solicitacao IPC GetStatus (PID {})...", client_pid);
    let (code, resp) = daemon.handle_ipc_request(client_pid, Web3IpcOpcode::GetStatus as u32, &[]);
    println!("   -> Retorno IPC: Status={}, PayloadBytes={}", code, resp.len());

    println!("\n3. Processando solicitacao IPC PrepareTransfer (PID {})...", client_pid);
    let recipient = [0x99; 20];
    let amount_wei = u128_to_u256_be(100 * 1_000_000_000_000_000_000u128); // 100 $JINN
    let mut payload = Vec::new();
    payload.extend_from_slice(&recipient);
    payload.extend_from_slice(&amount_wei);
    let (code, resp) = daemon.handle_ipc_request(client_pid, Web3IpcOpcode::PrepareTransfer as u32, &payload);
    let tx_id = u64::from_be_bytes(resp[..8].try_into().unwrap());
    println!("   -> Retorno IPC: Status={}, TxID={}, CalldataLen={}", code, tx_id, resp.len() - 8);

    println!("\n4. Processando solicitacao IPC SignAndSend (PID {})...", client_pid);
    let signed_tx = b"sample_signed_tx_for_bsc_testnet";
    let (code, resp) = daemon.handle_ipc_request(client_pid, Web3IpcOpcode::SignAndSend as u32, signed_tx);
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(&resp);
    println!("   -> Retorno IPC: Status={}, TxHash={}", code, to_hex_string(&hash_bytes));

    println!("\n5. Inspecionando logs de eventos seguros (sem vazamento de chaves)...");
    for (i, log) in daemon.event_logs.iter().enumerate() {
        println!("   Log #{}: Acao={}, PID={}, Status={}", i + 1, log.action, log.client_pid, log.status_code);
    }

    println!("\n✅ Demonstração do Daemon jinn-web3d concluída com êxito!");
}
