use anyhow::Result;
use chrono::Utc;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    prelude::*,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{
        transaction::eip2718::TypedTransaction, 
        Eip1559TransactionRequest, TransactionReceipt, BlockNumber,
        H256, U256
    },
};
use std::{env, fs, io::Write, path::Path, sync::Arc, time::Instant};
use tokio::time::sleep;
use std::time::Duration;

/// Sends an EIP-1559 transaction and waits for the receipt
/// Uses max fee per gas and priority fee per gas for more predictable inclusion
async fn send_and_confirm_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    nonce: u64,
    max_fee_per_gas: U256,
    priority_fee_per_gas: U256,
) -> Result<(H256, Duration, Duration)> {
    let address = client.address();
    
    // Create an EIP-1559 transaction
    let tx_request = Eip1559TransactionRequest::new()
        .to(address)
        .value(U256::zero())
        .nonce(nonce)
        .gas(21000) // Fixed gas limit for a simple ETH transfer
        .max_fee_per_gas(max_fee_per_gas)
        .max_priority_fee_per_gas(priority_fee_per_gas);
    
    // Convert to TypedTransaction
    let tx = TypedTransaction::Eip1559(tx_request);
    
    // Start measuring send time
    let send_start = Instant::now();
    
    // Send transaction
    let pending_tx = client.send_transaction(tx, None).await?;
    let tx_hash = pending_tx.tx_hash();
    
    // Measure send time
    let send_duration = send_start.elapsed();
    println!("TX sent in {:?}, hash: {}", send_duration, tx_hash);
    
    // Start measuring confirmation time
    let confirm_start = Instant::now();
    
    // Wait for receipt
    println!("Waiting for confirmation...");
    let mut receipt: Option<TransactionReceipt> = None;
    
    while receipt.is_none() {
        match client.get_transaction_receipt(tx_hash).await? {
            Some(r) => {
                receipt = Some(r);
                break;
            }
            None => {
                // Short sleep to avoid hammering the RPC
                sleep(Duration::from_millis(10)).await;
            }
        }
    }
    
    // Measure confirmation time
    let confirm_duration = confirm_start.elapsed();
    println!("TX confirmed in {:?}", confirm_duration);
    
    // Get block information
    if let Some(r) = receipt {
        if let Some(block_number) = r.block_number {
            println!("Included in block: {}", block_number);
        }
    }
    
    Ok((tx_hash, send_duration, confirm_duration))
}

/// Generates a markdown report of test results
fn generate_report(
    test_name: &str,
    rpc_url: &str,
    chain_id: U256,
    wallet_address: &str,
    base_fee: U256,
    max_fee_per_gas: U256,
    priority_fee_per_gas: U256,
    total_duration: Duration,
    results: &[(H256, Duration, Duration, Duration)],
) -> Result<String> {
    let timestamp = Utc::now().format("%Y-%m-%d-%H%M%S");
    let filename = if test_name.is_empty() {
        format!("rpc-test-{}.md", timestamp)
    } else {
        format!("{}-{}.md", test_name, timestamp)
    };
    
    let path = Path::new("results").join(&filename);
    
    // Create statistics
    let (min_send, max_send, avg_send, 
         min_confirm, max_confirm, avg_confirm,
         min_total, max_total, avg_total) = if !results.is_empty() {
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
        
        (*min_send, *max_send, avg_send,
         *min_confirm, *max_confirm, avg_confirm,
         *min_total, *max_total, avg_total)
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0, 0)
    };
    
    // Create markdown content
    let mut md_content = String::new();
    
    // Title and testing information
    md_content.push_str(&format!("# RPC Latency Test Results: {}\n\n", 
        if test_name.is_empty() { "Default" } else { test_name }));
    
    md_content.push_str(&format!("## Test Information\n\n"));
    md_content.push_str(&format!("- **Date and Time**: {}\n", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    md_content.push_str(&format!("- **RPC URL**: {}\n", rpc_url));
    md_content.push_str(&format!("- **Chain ID**: {}\n", chain_id));
    md_content.push_str(&format!("- **Wallet**: {}\n", wallet_address));
    md_content.push_str(&format!("- **Total Test Duration**: {} ms\n", total_duration.as_millis()));
    md_content.push_str(&format!("- **Number of Transactions**: {}\n", results.len()));
    md_content.push_str(&format!("- **Transaction Type**: EIP-1559 (Type 2)\n"));
    md_content.push_str(&format!("- **Base Fee**: {:.2} gwei\n", base_fee.as_u128() as f64 / 1_000_000_000.0));
    md_content.push_str(&format!("- **Max Fee Per Gas**: {:.2} gwei\n", max_fee_per_gas.as_u128() as f64 / 1_000_000_000.0));
    md_content.push_str(&format!("- **Priority Fee Per Gas**: {:.2} gwei\n\n", priority_fee_per_gas.as_u128() as f64 / 1_000_000_000.0));
    
    // Summary statistics
    md_content.push_str("## Summary Statistics\n\n");
    md_content.push_str("| Metric | Min (ms) | Max (ms) | Avg (ms) |\n");
    md_content.push_str("|--------|----------|----------|----------|\n");
    md_content.push_str(&format!("| Send Time | {} | {} | {} |\n", min_send, max_send, avg_send));
    md_content.push_str(&format!("| Confirm Time | {} | {} | {} |\n", min_confirm, max_confirm, avg_confirm));
    md_content.push_str(&format!("| Total Time | {} | {} | {} |\n\n", min_total, max_total, avg_total));
    
    // Individual transactions
    md_content.push_str("## Individual Transaction Results\n\n");
    md_content.push_str("| TX# | Send (ms) | Confirm (ms) | Total (ms) | Hash |\n");
    md_content.push_str("|-----|-----------|--------------|------------|--------------|\n");
    
    for (i, (hash, send_time, confirm_time, total_time)) in results.iter().enumerate() {
        md_content.push_str(&format!("| {} | {} | {} | {} | `0x{}` |\n", 
            i + 1,
            send_time.as_millis(),
            confirm_time.as_millis(),
            total_time.as_millis(),
            // Convert the full hash to a hex string without truncation
            hex::encode(hash.as_bytes())
        ));
    }
    
    // Create directory if it doesn't exist
    if !Path::new("results").exists() {
        fs::create_dir("results")?;
    }
    
    // Write to file
    let mut file = fs::File::create(&path)?;
    file.write_all(md_content.as_bytes())?;
    
    println!("\nReport saved to: {}", path.display());
    
    Ok(filename)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Check for test name from command line args
    let args: Vec<String> = std::env::args().collect();
    let test_name = if args.len() > 1 { &args[1] } else { "" };
    
    // Setup connection
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");
    
    let rpc_url_display = rpc_url.clone();
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let wallet: LocalWallet = private_key.parse()?;
    let wallet_address = wallet.address();
    let chain_id = provider.get_chainid().await?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
    // Make necessary RPC calls before the transaction loop
    let starting_nonce = client.get_transaction_count(wallet_address, None).await?.as_u64();
    
    // Get EIP-1559 fee data
    println!("Getting latest fee data from network...");
    
    // Get fee history to better estimate fees
    let fee_history = client.fee_history(10, BlockNumber::Latest, &[25.0, 50.0, 75.0]).await?;
    
    // Get latest base fee
    let latest_block = client.get_block(BlockNumber::Latest).await?;
    let base_fee = latest_block
        .and_then(|b| b.base_fee_per_gas)
        .unwrap_or_else(|| U256::from(1_000_000_000u64)); // Fallback to 1 gwei if not available
    
    // Use an aggressive priority fee 
    // Either from fee history or a minimum of 2 gwei (but significantly higher for fast inclusion)
    let suggested_priority_fee: U256 = fee_history.reward
        .last()
        .and_then(|r| r.last())
        .copied()
        .unwrap_or_else(|| U256::from(2_000_000_000u64)); // 2 gwei as fallback
    
    // Set an aggressive priority fee (3x the suggested or at least 5 gwei, but not more than 30 gwei)
    let min_priority_fee: U256 = U256::from(5_000_000_000u64); // 5 gwei
    let max_priority_fee: U256 = U256::from(30_000_000_000u64); // 30 gwei
    let priority_fee_per_gas: U256 = std::cmp::min(
        std::cmp::max(suggested_priority_fee * 3, min_priority_fee),
        max_priority_fee
    );
    
    // Calculate max fee per gas (base fee + priority fee + buffer)
    // Ensure max fee is always higher than priority fee
    let additional_buffer: U256 = U256::from(2_000_000_000u64); // 2 gwei buffer
    let max_fee_per_gas: U256 = base_fee + priority_fee_per_gas + additional_buffer;
    
    // Display info
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address: {}", wallet_address);
    println!("Starting nonce: {}", starting_nonce);
    println!("EIP-1559 Fee Data:");
    println!("  Base fee: {:.2} gwei", base_fee.as_u128() as f64 / 1_000_000_000.0);
    println!("  Max fee per gas: {:.2} gwei", max_fee_per_gas.as_u128() as f64 / 1_000_000_000.0);
    println!("  Priority fee per gas: {:.2} gwei", priority_fee_per_gas.as_u128() as f64 / 1_000_000_000.0);
    if !test_name.is_empty() {
        println!("Test name: {}", test_name);
    }
    
    // Start timer for entire batch
    let batch_start_time = Instant::now();
    
    // Get number of transactions from args or use default
    let num_transactions = if args.len() > 2 {
        match args[2].parse::<u64>() {
            Ok(n) => n,
            Err(_) => 10, // Default to 5 if parsing fails
        }
    } else {
        10 // Default to 10 transactions
    };
    
    println!("\nSending {} transactions sequentially, waiting for confirmation after each...", num_transactions);
    
    let mut results = Vec::with_capacity(num_transactions as usize);
    
    for i in 0..num_transactions {
        let nonce = starting_nonce + i;
        
        println!("\n--- Transaction #{} (nonce: {}) ---", i + 1, nonce);
        
        // Start timing total transaction time
        let tx_start = Instant::now();
        
        match send_and_confirm_transaction(client.clone(), nonce, max_fee_per_gas, priority_fee_per_gas).await {
            Ok((hash, send_time, confirm_time)) => {
                let total_time = tx_start.elapsed();
                println!("TX #{}: total time: {:?} (send: {:?}, confirm: {:?})", 
                         i + 1, total_time, send_time, confirm_time);
                
                results.push((hash, send_time, confirm_time, total_time));
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
             "TX#", "SEND (ms)", "CONFIRM (ms)", "TOTAL (ms)", "HASH");
    println!("{}", "-".repeat(120));
    
    for (i, (hash, send_time, confirm_time, total_time)) in results.iter().enumerate() {
        println!("{:<5} {:<12} {:<12} {:<12} {:<64}", 
                 i + 1,
                 send_time.as_millis(),
                 confirm_time.as_millis(),
                 total_time.as_millis(),
                 hash);
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
        
        // Generate markdown report
        match generate_report(
            test_name, 
            &rpc_url_display, 
            chain_id, 
            &wallet_address.to_string(),
            base_fee,
            max_fee_per_gas,
            priority_fee_per_gas,
            batch_elapsed, 
            &results
        ) {
            Ok(filename) => println!("Report generated: results/{}", filename),
            Err(e) => println!("Failed to generate report: {}", e),
        }
    }
    
    Ok(())
}