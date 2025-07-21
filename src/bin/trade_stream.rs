use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::io::stdout;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Clone)]
struct Trade {
    market: String,
    amount: f64,
    price: f64,
    is_buy: bool,
    timestamp: DateTime<Utc>,
    taker: String,
    maker: String,
}

impl Trade {
    fn new(market: String, price: f64) -> Self {
        let mut rng = thread_rng();
        
        Trade {
            market,
            amount: rng.gen_range(0.001..100.0),
            price,
            is_buy: rng.gen_bool(0.5),
            timestamp: Utc::now(),
            taker: format!("0x{}", generate_eth_address()),
            maker: format!("0x{}", generate_eth_address()),
        }
    }
    
    fn display_line(&self) -> String {
        let side_color = if self.is_buy { "BUY " } else { "SELL" };
        format!(
            "{} | {:>10} | {:>8.4} | {:>10.2} | {} | {} | {}",
            self.timestamp.format("%H:%M:%S%.3f"),
            self.market,
            self.amount,
            self.price,
            side_color,
            &self.taker[..10],
            &self.maker[..10]
        )
    }
}

fn generate_eth_address() -> String {
    let mut rng = thread_rng();
    (0..40)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

struct PriceTracker {
    prices: HashMap<String, f64>,
    drift_rate: f64,
}

impl PriceTracker {
    fn new() -> Self {
        let mut prices = HashMap::new();
        prices.insert("BTC/USDC".to_string(), 110000.0);
        prices.insert("ETH/USDC".to_string(), 2700.0);
        prices.insert("RISE/USDC".to_string(), 4.2);
        
        Self {
            prices,
            drift_rate: 0.001, // 0.1% max drift per update
        }
    }
    
    fn update_and_get_price(&mut self, market: &str) -> f64 {
        let mut rng = thread_rng();
        if let Some(price) = self.prices.get_mut(market) {
            // Random walk with uniform distribution
            let drift = rng.gen_range(-self.drift_rate..self.drift_rate);
            *price *= 1.0 + drift;
            *price
        } else {
            100.0 // Default price
        }
    }
    
    fn get_markets(&self) -> Vec<String> {
        self.prices.keys().cloned().collect()
    }
}

struct TradeStream {
    target_tps: f64,
    volatility: f64,
    trades: Vec<Trade>,
    total_trades: u64,
    start_time: Instant,
    price_tracker: PriceTracker,
}

impl TradeStream {
    fn new(target_tps: f64, volatility: f64) -> Self {
        Self {
            target_tps,
            volatility,
            trades: Vec::with_capacity(1000),
            total_trades: 0,
            start_time: Instant::now(),
            price_tracker: PriceTracker::new(),
        }
    }
    
    fn get_current_tps(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_trades as f64 / elapsed
        } else {
            0.0
        }
    }
    
    fn generate_trades(&mut self, count: usize) {
        for _ in 0..count {
            let mut rng = thread_rng();
            let markets = self.price_tracker.get_markets();
            let market = markets[rng.gen_range(0..markets.len())].clone();
            let price = self.price_tracker.update_and_get_price(&market);
            
            let trade = Trade::new(market, price);
            self.trades.push(trade);
            self.total_trades += 1;
            
            // Keep only last 100 trades for display
            if self.trades.len() > 100 {
                self.trades.remove(0);
            }
        }
    }
    
    fn get_trades_per_batch(&self) -> usize {
        let mut rng = thread_rng();
        let variation = rng.gen_range(-self.volatility..self.volatility);
        let adjusted_tps = self.target_tps * (1.0 + variation);
        
        // Generate trades in small batches (10ms intervals)
        ((adjusted_tps / 100.0).max(1.0)) as usize
    }
}

async fn run_trade_stream(target_tps: f64, volatility: f64) -> Result<()> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide)?;
    
    let mut stream = TradeStream::new(target_tps, volatility);
    let mut last_update = Instant::now();
    
    loop {
        // Check for exit key
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }
        
        // Generate trades
        let batch_size = stream.get_trades_per_batch();
        stream.generate_trades(batch_size);
        
        // Update display every 50ms
        if last_update.elapsed() >= Duration::from_millis(50) {
            draw_ui(&stream)?;
            last_update = Instant::now();
        }
        
        // Sleep for 10ms between batches
        sleep(Duration::from_millis(10)).await;
    }
    
    execute!(stdout(), Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    
    Ok(())
}

fn draw_ui(stream: &TradeStream) -> Result<()> {
    let (width, height) = terminal::size()?;
    
    // Clear screen
    execute!(stdout(), Clear(ClearType::All))?;
    
    // Draw telemetry box
    draw_telemetry_box(stream, width)?;
    
    // Draw trade log
    draw_trade_log(stream, height)?;
    
    // Draw instructions
    execute!(
        stdout(),
        MoveTo(0, height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print("Press 'q' or ESC to quit"),
        ResetColor
    )?;
    
    Ok(())
}

fn draw_telemetry_box(stream: &TradeStream, _width: u16) -> Result<()> {
    let current_tps = stream.get_current_tps();
    let elapsed = stream.start_time.elapsed();
    
    // Box top
    execute!(stdout(), MoveTo(0, 0), Print("╔═══════════════════════════════════════════╗"))?;
    
    // Telemetry data
    execute!(
        stdout(),
        MoveTo(0, 1),
        Print(format!("║ Current Trades/s: {:>8.1}                ║", current_tps))
    )?;
    execute!(
        stdout(),
        MoveTo(0, 2),
        Print(format!("║ Total Trades: {:>12}                ║", stream.total_trades))
    )?;
    execute!(
        stdout(),
        MoveTo(0, 3),
        Print(format!("║ Runtime: {:>02}:{:02}:{:02}                         ║", 
            elapsed.as_secs() / 3600,
            (elapsed.as_secs() % 3600) / 60,
            elapsed.as_secs() % 60
        ))
    )?;
    
    // Box bottom
    execute!(stdout(), MoveTo(0, 4), Print("╚═══════════════════════════════════════════╝"))?;
    
    Ok(())
}

fn draw_trade_log(stream: &TradeStream, height: u16) -> Result<()> {
    // Header
    execute!(
        stdout(),
        MoveTo(0, 6),
        SetForegroundColor(Color::Cyan),
        Print("TIMESTAMP    |     MARKET |   AMOUNT |      PRICE | SIDE | TAKER      | MAKER"),
        ResetColor
    )?;
    execute!(stdout(), MoveTo(0, 7), Print("─".repeat(82)))?;
    
    // Trade entries
    let start_y = 8;
    let max_trades = (height - start_y - 2) as usize;
    let trades_to_show = stream.trades.len().min(max_trades);
    let start_idx = stream.trades.len().saturating_sub(trades_to_show);
    
    for (i, trade) in stream.trades[start_idx..].iter().enumerate() {
        let y = start_y + i as u16;
        execute!(stdout(), MoveTo(0, y))?;
        
        // Color based on buy/sell
        if trade.is_buy {
            execute!(stdout(), SetForegroundColor(Color::Green))?;
        } else {
            execute!(stdout(), SetForegroundColor(Color::Red))?;
        }
        
        execute!(stdout(), Print(trade.display_line()), ResetColor)?;
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let target_tps = std::env::var("TARGET_TPS")
        .unwrap_or_else(|_| "1000".to_string())
        .parse::<f64>()
        .unwrap_or(100.0);
        
    let volatility = std::env::var("VOLATILITY")
        .unwrap_or_else(|_| "0.3".to_string())
        .parse::<f64>()
        .unwrap_or(0.3)
        .clamp(0.0, 1.0);
    
    // println!("Starting trade stream with target TPS: {} and volatility: {}%", target_tps, volatility * 100.0);
    // println!("Press any key to continue...");
    // let _ = std::io::stdin().read_line(&mut String::new());
    
    run_trade_stream(target_tps, volatility).await?;
    
    Ok(())
}