use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{StreamExt, SinkExt};
use serde_json::{json, Value};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let connect_addr = "wss://staging.riselabs.xyz/ws";
    println!("Connecting to {}", connect_addr);

    let (ws_stream, _) = connect_async(connect_addr).await?;
    println!("WebSocket connection established");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to the stream
    let subscribe_msg = json!({
        "method": "rise_subscribe",
        "params": [],
        "id": 1,
        "jsonrpc": "2.0",
    });
    write.send(Message::Text(subscribe_msg.to_string())).await?;
    println!("Subscription request sent");

    let mut last_shred_time = get_timestamp_ms();

    // Handle incoming messages
    while let Some(message) = read.next().await {
        let now = get_timestamp_ms();
        match message {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<Value>(&text) {
                    Ok(json) => {
                        if let Some(result) = json.get("params").and_then(|p| p.get("result")) {
                            if let (Some(block_number), Some(shred_idx)) = (
                                result.get("block_number").and_then(|n| n.as_u64()),
                                result.get("shred_idx").and_then(|i| i.as_u64()),
                            ) {
                                let interval = now - last_shred_time;
                                println!("Block Number: {}", block_number);
                                println!("Shred Index: {}", shred_idx);
                                println!("Shred Interval: {}ms", interval);
                                println!("Shred Content: {}", serde_json::to_string_pretty(result)?);
                                last_shred_time = now;
                            }
                        }
                    },
                    Err(e) => eprintln!("Error parsing JSON: {}", e),
                }
            },
            Ok(Message::Close(_)) => {
                println!("Connection closed");
                break;
            },
            Ok(_) => {},
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn get_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}