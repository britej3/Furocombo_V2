//! main.rs - Entry point for the Furucombo Metis Arbitrage Bot
//!
//! Phase 1: Real Metis Price Feeds
//! - Fetches trading pairs from Netswap and Tethys on Metis
//! - Displays available pairs and their prices
//! - Runs periodic refresh loop

use furucombo_arbitrage::{MetisPriceFeed, PriceFeed, NAME, VERSION};
use log::{debug, error, info, warn};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

/// Scan interval in seconds
const SCAN_INTERVAL_SECONDS: u64 = 30;

/// Minimum liquidity threshold in USD
const MIN_LIQUIDITY_USD: u64 = 5000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    // Print startup banner
    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     🚀 Furucombo Metis Arbitrage Bot v{}              ║", VERSION);
    println!("║     Phase 1: Real Metis Price Feeds                      ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    info!("Starting {} v{}", NAME, VERSION);
    info!("Phase 1: Real Metis Price Feeds (Netswap + Tethys)");

    // Initialize the price feed
    // Phase 1: Using MetisPriceFeed (real data from DEX Screener)
    let price_feed: Arc<dyn PriceFeed + Send + Sync> = Arc::new(MetisPriceFeed::new());

    info!("✓ Price feed initialized: MetisPriceFeed");
    info!("  - DEX Screener API: https://api.dexscreener.com/latest/dex");
    info!("  - Supported DEXes: Netswap, Tethys");
    info!("  - Chain: Metis (Chain ID: 1088)");
    println!();

    // Initial data fetch
    info!("📊 Fetching initial market data...");
    match price_feed.refresh().await {
        Ok(_) => info!("✓ Initial data fetch successful"),
        Err(e) => {
            error!("✗ Initial data fetch failed: {}", e);
            warn!("Will retry in the main loop...");
        }
    }

    // Display initial pairs
    display_trading_pairs(&price_feed).await;

    // Main scanning loop
    info!("🔄 Starting scan loop (interval: {}s)...", SCAN_INTERVAL_SECONDS);
    println!();

    let mut interval = tokio::time::interval(Duration::from_secs(SCAN_INTERVAL_SECONDS));
    let mut scan_count: u64 = 0;

    loop {
        interval.tick().await;
        scan_count += 1;

        debug!("─────────────────────────────────────────────────────────────");
        info!("📡 Scan #{}: Refreshing price data...", scan_count);

        // Refresh price data
        match price_feed.refresh().await {
            Ok(_) => {
                let pairs = price_feed.get_trading_pairs().await;
                info!("✓ Scan #{} complete: {} pairs available", scan_count, pairs.len());

                // Find potential arbitrage opportunities (Phase 1: just detect price differences)
                find_price_differences(&pairs).await;
            }
            Err(e) => {
                error!("✗ Scan #{} failed: {}", scan_count, e);
            }
        }

        // Stats every 10 scans
        if scan_count % 10 == 0 {
            info!("📈 Stats: {} scans completed", scan_count);
        }
    }
}

/// Display all available trading pairs
async fn display_trading_pairs(price_feed: &Arc<dyn PriceFeed + Send + Sync>) {
    let pairs = price_feed.get_trading_pairs().await;

    if pairs.is_empty() {
        warn!("No trading pairs found!");
        return;
    }

    println!();
    println!("┌────────────────────────────────────────────────────────────────────────────────┐");
    println!("│                          Available Trading Pairs                               │");
    println!("├──────────────────┬──────────────┬────────────────┬─────────────────────────────┤");
    println!("│ Pair             │ Exchange     │ Price (USD)    │ Liquidity (USD)             │");
    println!("├──────────────────┼──────────────┼────────────────┼─────────────────────────────┤");

    let min_liquidity = Decimal::from(MIN_LIQUIDITY_USD);
    let mut displayed = 0;

    for pair in pairs.iter().filter(|p| p.liquidity >= min_liquidity) {
        println!(
            "│ {:16} │ {:12} │ {:>14.4} │ {:>27.2} │",
            format!("{}/{}", pair.base_token.symbol, pair.quote_token.symbol),
            pair.exchange.name,
            pair.price,
            pair.liquidity
        );
        displayed += 1;

        if displayed >= 20 {
            println!("│ ... and {} more pairs                                                         │",
                pairs.len() - displayed);
            break;
        }
    }

    println!("└──────────────────┴──────────────┴────────────────┴─────────────────────────────┘");
    println!();
    info!("Total pairs: {} (showing {} with liquidity >= ${})",
        pairs.len(), displayed, MIN_LIQUIDITY_USD);
}

/// Find price differences between the same pair on different DEXes
/// This is a simplified Phase 1 implementation - just detection, no execution
async fn find_price_differences(pairs: &[furucombo_arbitrage::TradingPair]) {
    use std::collections::HashMap;

    // Group pairs by token pair (base/quote)
    let mut pair_groups: HashMap<String, Vec<&furucombo_arbitrage::TradingPair>> = HashMap::new();

    for pair in pairs {
        let key = format!("{}/{}", pair.base_token.symbol, pair.quote_token.symbol);
        pair_groups.entry(key).or_default().push(pair);
    }

    // Find pairs listed on multiple DEXes
    let mut opportunities_found = 0;

    for (pair_id, exchanges) in pair_groups.iter() {
        if exchanges.len() < 2 {
            continue; // Need at least 2 DEXes for arbitrage
        }

        // Find min and max prices
        let prices: Vec<(Decimal, &str)> = exchanges
            .iter()
            .map(|p| (p.price, p.exchange.name.as_str()))
            .collect();

        let (min_price, min_exchange) = prices.iter().min_by(|a, b| a.0.cmp(&b.0)).unwrap();
        let (max_price, max_exchange) = prices.iter().max_by(|a, b| a.0.cmp(&b.0)).unwrap();

        // Calculate spread
        if *min_price > Decimal::ZERO {
            let spread = ((*max_price - *min_price) / *min_price) * Decimal::from(100);

            // Only report significant spreads (> 0.5%)
            if spread > Decimal::from_str("0.5").unwrap() {
                opportunities_found += 1;

                info!(
                    "💡 Potential opportunity: {} | Spread: {:.2}%",
                    pair_id, spread
                );
                info!(
                    "   Buy on {} @ ${:.4} → Sell on {} @ ${:.4}",
                    min_exchange, min_price, max_exchange, max_price
                );
            }
        }
    }

    if opportunities_found == 0 {
        debug!("No significant price differences detected this scan");
    } else {
        info!("🎯 {} potential opportunities detected", opportunities_found);
    }
}
