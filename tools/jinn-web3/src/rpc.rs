//! EVM JSON-RPC 2.0 Request and Response Engine for Jinn OS
//! Generates and parses zero-allocation payloads formatted for HTTP POST over Jinn sockets.

#![allow(dead_code)]

use crate::bep20::to_hex_string;

#[derive(Debug, Clone)]
pub struct RpcResponse {
    pub id: u64,
    pub result: Option<String>,
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
}

/// Formats a JSON-RPC 2.0 `eth_blockNumber` request.
pub fn format_eth_block_number(id: u64) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":{}}}",
        id
    )
}

/// Formats a JSON-RPC 2.0 `eth_chainId` request to verify BSC network (97 or 56).
pub fn format_eth_chain_id(id: u64) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_chainId\",\"params\":[],\"id\":{}}}",
        id
    )
}

/// Formats a JSON-RPC 2.0 `eth_getBalance` request (BNB native balance).
pub fn format_eth_get_balance(address: &[u8; 20], id: u64) -> String {
    let addr_hex = to_hex_string(address);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"{}\",\"latest\"],\"id\":{}}}",
        addr_hex, id
    )
}

/// Formats a JSON-RPC 2.0 `eth_getTransactionCount` request to retrieve account nonce.
pub fn format_eth_get_nonce(address: &[u8; 20], id: u64) -> String {
    let addr_hex = to_hex_string(address);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_getTransactionCount\",\"params\":[\"{}\",\"pending\"],\"id\":{}}}",
        addr_hex, id
    )
}

/// Formats a JSON-RPC 2.0 `eth_estimateGas` request.
pub fn format_eth_estimate_gas(to: &[u8; 20], calldata: &[u8], id: u64) -> String {
    let to_hex = to_hex_string(to);
    let data_hex = to_hex_string(calldata);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_estimateGas\",\"params\":[{{\"to\":\"{}\",\"data\":\"{}\"}}],\"id\":{}}}",
        to_hex, data_hex, id
    )
}

/// Formats a JSON-RPC 2.0 `eth_call` request for BEP-20 queries (e.g. `balanceOf`, `totalSupply`).
pub fn format_eth_call(to: &[u8; 20], calldata: &[u8], id: u64) -> String {
    let to_hex = to_hex_string(to);
    let data_hex = to_hex_string(calldata);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_call\",\"params\":[{{\"to\":\"{}\",\"data\":\"{}\"}},\"latest\"],\"id\":{}}}",
        to_hex, data_hex, id
    )
}

/// Formats a JSON-RPC 2.0 `eth_sendRawTransaction` request for broadcasting signed transactions.
pub fn format_eth_send_raw_transaction(raw_tx_bytes: &[u8], id: u64) -> String {
    let tx_hex = to_hex_string(raw_tx_bytes);
    format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"eth_sendRawTransaction\",\"params\":[\"{}\"],\"id\":{}}}",
        tx_hex, id
    )
}

/// Lightweight parser for extracting JSON-RPC `"result"` or `"error"` string from an HTTP response payload.
pub fn parse_json_rpc_result(body: &str) -> RpcResponse {
    let mut resp = RpcResponse {
        id: 0,
        result: None,
        error_code: None,
        error_message: None,
    };

    // Extract "id"
    if let Some(pos) = body.find("\"id\":") {
        let after = &body[pos + 5..];
        let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
        if let Ok(id_val) = after[..end].trim().parse::<u64>() {
            resp.id = id_val;
        }
    }

    // Check for error
    if let Some(pos) = body.find("\"error\":") {
        let err_part = &body[pos..];
        if let Some(msg_pos) = err_part.find("\"message\":\"") {
            let msg_after = &err_part[msg_pos + 11..];
            if let Some(msg_end) = msg_after.find('\"') {
                resp.error_message = Some(msg_after[..msg_end].to_string());
            }
        }
    } else if let Some(pos) = body.find("\"result\":") {
        let after = &body[pos + 9..].trim_start();
        if after.starts_with('\"') {
            let after_quote = &after[1..];
            if let Some(end_quote) = after_quote.find('\"') {
                resp.result = Some(after_quote[..end_quote].to_string());
            }
        } else {
            let end = after.find(|c: char| c == ',' || c == '}' || c == ']').unwrap_or(after.len());
            resp.result = Some(after[..end].trim().to_string());
        }
    }

    resp
}

/// Formats an HTTP/1.1 POST wrapper header for sending over a raw TCP socket in Jinn OS.
pub fn format_http_post(host: &str, json_payload: &str) -> String {
    format!(
        "POST / HTTP/1.1\r\n\
         Host: {}\r\n\
         User-Agent: JinnOS-Web3/1.0 (x86_64; Aladdin Desktop)\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        host,
        json_payload.len(),
        json_payload
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_rpc_result() {
        let payload = "{\"jsonrpc\":\"2.0\",\"id\":42,\"result\":\"0x00000000000000000000000000000000000000000000003635c9adc5dea00000\"}";
        let parsed = parse_json_rpc_result(payload);
        assert_eq!(parsed.id, 42);
        assert!(parsed.result.is_some());
        assert_eq!(
            parsed.result.unwrap(),
            "0x00000000000000000000000000000000000000000000003635c9adc5dea00000"
        );
    }
}
