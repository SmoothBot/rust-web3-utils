use anyhow::Result;
use dotenv::dotenv;
use solana_client::{rpc_client::RpcClient};
use solana_sdk::{
    signature::{Keypair, Signer, Signature},
    transaction::Transaction,
    system_instruction,
    commitment_config::CommitmentConfig,
};
use std::{env, time::Instant};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Setup connection
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");

    let rpc_url_display = rpc_url.clone();
    let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

    let keypair = Keypair::from_base58_string(&private_key);
    let wallet_address = keypair.pubkey();
    
    println!("RPC URL: {}", rpc_url_display);
    println!("Wallet address: {}", wallet_address);
    
    // Get balance to display
    match client.get_balance(&wallet_address) {
        Ok(balance) => println!("Wallet balance: {} lamports (≈{} SOL)", balance, balance as f64 / 1_000_000_000.0),
        Err(e) => println!("Failed to get balance: {}", e),
    }
    
    // Start timer for entire batch
    let batch_start_time = Instant::now();
    
    // Send 10 transactions sequentially, waiting for confirmation after each
    println!("\nSending 10 transactions sequentially, waiting for confirmation after each...");
    
    let mut results: Vec<(String, Duration, Duration, Duration)> = Vec::with_capacity(10);
    
    for i in 0..10 {
        // Start timing total transaction time
        let tx_start = Instant::now();
        
        match send_and_confirm_transaction(&client, &keypair, i as u64).await {
            Ok((signature, send_time, confirm_time)) => {
                let total_time = tx_start.elapsed();
                println!("TX #{}: total time: {:?} (send: {:?}, confirm: {:?})", 
                         i + 1, total_time, send_time, confirm_time);
                
                results.push((signature, send_time, confirm_time, total_time));
            },
            Err(e) => {
                println!("TX #{}: error: {}", i + 1, e);
            }
        }
        
        println!("--- End Transaction #{} ---\n", i + 1);
    }
    
    let batch_elapsed = batch_start_time.elapsed();
    
    // Print summary
    println!("\n===== SUMMARY =====");
    println!("Total time for all transactions: {:?}", batch_elapsed);
    println!();
    
    println!("Individual Transaction Results:");
    println!("{:<5} {:<12} {:<12} {:<12} {:<64}", 
             "TX#", "SEND (ms)", "CONFIRM (ms)", "TOTAL (ms)", "SIGNATURE");
    println!("{}", "-".repeat(120));
    
    for (i, (signature, send_time, confirm_time, total_time)) in results.iter().enumerate() {
        println!("{:<5} {:<12} {:<12} {:<12} {:<64}", 
                 i + 1,
                 send_time.as_millis(),
                 confirm_time.as_millis(),
                 total_time.as_millis(),
                 signature);
    }
    
    // Calculate min, max, and averages
    if !results.is_empty() {
        // Send time stats
        let send_times = results.iter().map(|(_, s, _, _)| s.as_millis() as u128).collect::<Vec<_>>();
        let min_send = send_times.iter().min().unwrap_or(&0);
        let max_send = send_times.iter().max().unwrap_or(&0);
        let avg_send = send_times.iter().sum::<u128>() / send_times.len() as u128;

        // Confirm time stats
        let confirm_times = results.iter().map(|(_, _, c, _)| c.as_millis() as u128).collect::<Vec<_>>();
        let min_confirm = confirm_times.iter().min().unwrap_or(&0);
        let max_confirm = confirm_times.iter().max().unwrap_or(&0);
        let avg_confirm = confirm_times.iter().sum::<u128>() / confirm_times.len() as u128;

        // Total time stats
        let total_times = results.iter().map(|(_, _, _, t)| t.as_millis() as u128).collect::<Vec<_>>();
        let min_total = total_times.iter().min().unwrap_or(&0);
        let max_total = total_times.iter().max().unwrap_or(&0);
        let avg_total = total_times.iter().sum::<u128>() / total_times.len() as u128;
        
        println!("\nLATENCY STATISTICS:");
        println!("{:<13} {:<10} {:<10} {:<10}", "", "MIN (ms)", "MAX (ms)", "AVG (ms)");
        println!("{}", "-".repeat(45));
        println!("{:<13} {:<10} {:<10} {:<10}", "Send time:", min_send, max_send, avg_send);
        println!("{:<13} {:<10} {:<10} {:<10}", "Confirm time:", min_confirm, max_confirm, avg_confirm);
        println!("{:<13} {:<10} {:<10} {:<10}", "Total time:", min_total, max_total, avg_total);
        
        println!("\nSUMMARY: {} transactions sent and confirmed sequentially in {} ms (min: {} ms, max: {} ms, avg: {} ms)",
            results.len(), batch_elapsed.as_millis(), min_total, max_total, avg_total);
    }
    
    Ok(())
}

/// Sends a transaction and waits for the receipt
/// This version removes unnecessary await calls to minimize RPC requests
async fn send_and_confirm_transaction(
    client: &RpcClient,
    keypair: &Keypair,
    nonce: u64,
) -> Result<(String, Duration, Duration)> {
    let address = keypair.pubkey();
    
    // Get a fresh blockhash for each transaction
    let blockhash = client.get_latest_blockhash()?;
    
    // Construct a simple transfer transaction (send minimal lamports to the wallet address)
    let tx = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(&address, &address, 100)], // transfer 0.0000001 SOL
        Some(&address),
        &[keypair],
        blockhash,
    );
    
    // Start measuring send time
    let send_start = Instant::now();
    
    // Send transaction
    let signature = client.send_and_confirm_transaction_with_spinner_and_commitment(
        &tx,
        CommitmentConfig::confirmed(),
    )?;
    
    // Measure send time
    let send_duration = send_start.elapsed();
    println!("TX sent in {:?}, signature: {}", send_duration, signature);
    
    // Start measuring confirmation time
    let confirm_start = Instant::now();
    
    // Wait for confirmation (actually already done by send_and_confirm_transaction_with_spinner_and_commitment)
    let confirm_duration = confirm_start.elapsed();
    println!("TX confirmed in {:?}", confirm_duration);
    
    // Return transaction signature as string (not trying to convert to Hash), send time, and confirmation time
    Ok((signature.to_string(), send_duration, confirm_duration))
}