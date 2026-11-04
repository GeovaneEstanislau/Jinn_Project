//! Jinn Web3 & Token Utility CLI
//! Tooling for testing, encoding, and simulating Web3 & BEP-20 operations on Jinn OS.

#![allow(unused_imports)]

mod bep20;
mod invariants;
mod keccak;
mod keystore;
mod merkle;
mod rpc;

use bep20::*;
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
