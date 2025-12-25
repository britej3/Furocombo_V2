//! price_feed.rs - Price feed implementations for the arbitrage engine
//!
//! Phase 1: Real Metis price feeds from DEX Screener API
//! Supports Netswap and Tethys DEXes on Metis chain

use async_trait::async_trait;
use log::{debug, error, info, warn};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::models::{CachedPrice, Exchange, Token, TradingPair};

/// Trait defining the interface for price feeds
#[async_trait]
pub trait PriceFeed: Send + Sync {
    /// Get all available trading pairs
    async fn get_trading_pairs(&self) -> Vec<TradingPair>;

    /// Get price for a specific pair
    async fn get_price(&self, base: &str, quote: &str) -> Option<Decimal>;

    /// Get liquidity for a specific pair
    async fn get_liquidity(&self, base: &str, quote: &str) -> Option<Decimal>;

    /// Refresh all price data
    async fn refresh(&self) -> anyhow::Result<()>;
}

// ============================================================================
// DEX Screener API Response Structures
// ============================================================================

#[derive(Debug, Deserialize)]
struct DexScreenerResponse {
    pairs: Option<Vec<DexScreenerPair>>,
}

#[derive(Debug, Deserialize)]
struct DexScreenerPair {
    #[serde(rename = "chainId")]
    chain_id: String,

    #[serde(rename = "dexId")]
    dex_id: String,

    #[serde(rename = "pairAddress")]
    pair_address: String,

    #[serde(rename = "baseToken")]
    base_token: TokenData,

    #[serde(rename = "quoteToken")]
    quote_token: TokenData,

    #[serde(rename = "priceUsd")]
    price_usd: Option<String>,

    #[serde(rename = "priceNative")]
    price_native: Option<String>,

    liquidity: Option<LiquidityData>,
}

#[derive(Debug, Deserialize)]
struct TokenData {
    address: String,
    name: String,
    symbol: String,
}

#[derive(Debug, Deserialize)]
struct LiquidityData {
    usd: Option<f64>,
    base: Option<f64>,
    quote: Option<f64>,
}

// ============================================================================
// MetisPriceFeed - Real price feed for Metis chain
// ============================================================================

/// Real price feed implementation for Metis chain
/// Fetches data from DEX Screener API for Netswap and Tethys DEXes
#[derive(Debug, Clone)]
pub struct MetisPriceFeed {
    client: Client,
    dex_screener_url: String,
    cache: Arc<RwLock<HashMap<String, CachedPrice>>>,
    pairs_cache: Arc<RwLock<Vec<TradingPair>>>,
}

impl MetisPriceFeed {
    /// Create a new MetisPriceFeed instance
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        MetisPriceFeed {
            client,
            dex_screener_url: "https://api.dexscreener.com/latest/dex".to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            pairs_cache: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Fetch trading pairs from Metis DEXes (Netswap and Tethys)
    async fn fetch_metis_pairs(&self) -> Result<Vec<TradingPair>, anyhow::Error> {
        let mut all_pairs = Vec::new();

        // Use search endpoint to find Metis chain pairs
        // Search for common Metis tokens to get pairs
        let search_terms = vec!["metis", "netswap", "tethys"];

        for term in search_terms {
            let url = format!("{}/search?q={}", self.dex_screener_url, term);

            debug!("Fetching from: {}", url);

            let response = match self.client.get(&url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Failed to fetch for term '{}': {}", term, e);
                    continue;
                }
            };

            if !response.status().is_success() {
                warn!("DEX Screener returned status {} for '{}'", response.status(), term);
                continue;
            }

            let data: DexScreenerResponse = match response.json().await {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to parse response for '{}': {}", term, e);
                    continue;
                }
            };

            let pairs = data.pairs.unwrap_or_default();
            debug!("Found {} pairs for search term '{}'", pairs.len(), term);

            // Filter for Metis chain only (chainId == "metis") and supported DEXes
            for pair_data in pairs {
                // Only include Metis chain pairs from Netswap or Tethys
                if pair_data.chain_id != "metis" {
                    continue;
                }
                if pair_data.dex_id != "netswap" && pair_data.dex_id != "tethys" {
                    continue;
                }

                match self.convert_to_trading_pair(pair_data) {
                    Ok(pair) => {
                        // Avoid duplicates
                        if !all_pairs.iter().any(|p: &TradingPair| p.full_id() == pair.full_id()) {
                            all_pairs.push(pair);
                        }
                    }
                    Err(e) => {
                        // Don't log every conversion failure, too noisy
                        debug!("Skipped pair conversion: {}", e);
                    }
                }
            }
        }

        info!("Total pairs fetched: {}", all_pairs.len());
        Ok(all_pairs)
    }

    /// Convert DEX Screener pair data to our TradingPair model
    fn convert_to_trading_pair(
        &self,
        data: DexScreenerPair,
    ) -> Result<TradingPair, anyhow::Error> {
        // Extract price - use priceUsd if available, otherwise priceNative
        let price_str = data.price_usd
            .or(data.price_native)
            .ok_or_else(|| anyhow::anyhow!("No price data"))?;

        let price = Decimal::from_str(&price_str)
            .unwrap_or(Decimal::ZERO);

        // Skip pairs with zero or invalid price
        if price <= Decimal::ZERO {
            return Err(anyhow::anyhow!("Invalid price: {}", price_str));
        }

        // Extract liquidity data
        let liquidity = data.liquidity.as_ref();
        let liquidity_usd = liquidity
            .and_then(|l| l.usd)
            .map(|v| Decimal::from_str(&v.to_string()).unwrap_or(Decimal::ZERO))
            .unwrap_or(Decimal::ZERO);

        // Skip pairs with very low liquidity (< $1000)
        if liquidity_usd < Decimal::from(1000) {
            return Err(anyhow::anyhow!("Liquidity too low: ${}", liquidity_usd));
        }

        let reserve_base = liquidity
            .and_then(|l| l.base)
            .map(|v| Decimal::from_str(&v.to_string()).unwrap_or(Decimal::ZERO))
            .unwrap_or(Decimal::ZERO);

        let reserve_quote = liquidity
            .and_then(|l| l.quote)
            .map(|v| Decimal::from_str(&v.to_string()).unwrap_or(Decimal::ZERO))
            .unwrap_or(Decimal::ZERO);

        // Create token models
        let base_token = Token::new(
            &data.base_token.symbol,
            &data.base_token.name,
            18, // Default to 18 decimals, will be refined in future phases
            &data.base_token.address,
        );

        let quote_token = Token::new(
            &data.quote_token.symbol,
            &data.quote_token.name,
            18,
            &data.quote_token.address,
        );

        // Create exchange model
        let exchange = Exchange::new(
            &data.dex_id,
            "Metis",
            &data.pair_address,
        );

        Ok(TradingPair::new(
            base_token,
            quote_token,
            exchange,
            price,
            liquidity_usd,
            reserve_base,
            reserve_quote,
        ))
    }
}

impl Default for MetisPriceFeed {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceFeed for MetisPriceFeed {
    async fn get_trading_pairs(&self) -> Vec<TradingPair> {
        // Check cache first
        {
            let cache = self.pairs_cache.read().await;
            if !cache.is_empty() {
                return cache.clone();
            }
        }

        // Fetch fresh data
        match self.fetch_metis_pairs().await {
            Ok(pairs) => {
                // Update cache
                let mut cache = self.pairs_cache.write().await;
                *cache = pairs.clone();
                pairs
            }
            Err(e) => {
                error!("Failed to fetch Metis pairs: {}", e);
                Vec::new()
            }
        }
    }

    async fn get_price(&self, base: &str, quote: &str) -> Option<Decimal> {
        let cache = self.cache.read().await;
        let key = format!("{}/{}", base, quote);

        if let Some(cached) = cache.get(&key) {
            if !cached.is_stale(60) {
                // 60 second cache
                return Some(cached.price);
            }
        }

        // Price not in cache, return None (caller should refresh)
        None
    }

    async fn get_liquidity(&self, base: &str, quote: &str) -> Option<Decimal> {
        let cache = self.cache.read().await;
        let key = format!("{}/{}-liq", base, quote);

        cache.get(&key).map(|c| c.price)
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        debug!("Refreshing Metis price feed...");

        let pairs = self.fetch_metis_pairs().await?;
        let mut price_cache = self.cache.write().await;
        let mut pairs_cache = self.pairs_cache.write().await;

        // Update pairs cache
        *pairs_cache = pairs.clone();

        // Update price cache
        for pair in pairs {
            // Cache price
            let price_key = format!("{}/{}",
                pair.base_token.symbol,
                pair.quote_token.symbol
            );

            price_cache.insert(price_key, CachedPrice {
                price: pair.price,
                timestamp: chrono::Utc::now(),
                source: format!("DEX Screener - {}", pair.exchange.name),
            });

            // Cache liquidity
            let liq_key = format!("{}/{}-liq",
                pair.base_token.symbol,
                pair.quote_token.symbol
            );

            price_cache.insert(liq_key, CachedPrice {
                price: pair.liquidity,
                timestamp: chrono::Utc::now(),
                source: format!("DEX Screener - {}", pair.exchange.name),
            });
        }

        info!("Price feed refreshed: {} entries cached", price_cache.len());
        Ok(())
    }
}

// ============================================================================
// MockPriceFeed - For testing purposes
// ============================================================================

/// Mock price feed for testing and development
#[derive(Debug, Clone)]
pub struct MockPriceFeed {
    pairs: Vec<TradingPair>,
}

impl MockPriceFeed {
    pub fn new() -> Self {
        // Create some mock trading pairs for testing
        let weth = Token::new("WETH", "Wrapped Ether", 18, "0x420000000000000000000000000000000000000a");
        let usdc = Token::new("USDC", "USD Coin", 6, "0xEA32A96608495e54156Ae48931A7c20f0dcc1a21");
        let metis = Token::new("METIS", "Metis Token", 18, "0xDeadDeAddeAddEAddeadDEaDDEAdDeaDDeAD0000");

        let netswap = Exchange::new("netswap", "Metis", "0x1E876cCe41B7b844FDe09E38Fa1cf00f213bFf56");
        let tethys = Exchange::new("tethys", "Metis", "0x81b9FA50D5f5155Ee17817C21702C3AE4780AD09");

        let pairs = vec![
            TradingPair::new(
                weth.clone(), usdc.clone(), netswap.clone(),
                Decimal::from(1850),
                Decimal::from(500000),
                Decimal::from(270),
                Decimal::from(500000),
            ),
            TradingPair::new(
                weth.clone(), usdc.clone(), tethys.clone(),
                Decimal::from(1852),
                Decimal::from(350000),
                Decimal::from(189),
                Decimal::from(350000),
            ),
            TradingPair::new(
                metis.clone(), usdc.clone(), netswap.clone(),
                Decimal::from(85),
                Decimal::from(200000),
                Decimal::from(2353),
                Decimal::from(200000),
            ),
            TradingPair::new(
                metis.clone(), usdc.clone(), tethys.clone(),
                Decimal::from(84),
                Decimal::from(150000),
                Decimal::from(1786),
                Decimal::from(150000),
            ),
        ];

        MockPriceFeed { pairs }
    }
}

impl Default for MockPriceFeed {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceFeed for MockPriceFeed {
    async fn get_trading_pairs(&self) -> Vec<TradingPair> {
        self.pairs.clone()
    }

    async fn get_price(&self, base: &str, quote: &str) -> Option<Decimal> {
        self.pairs.iter()
            .find(|p| p.base_token.symbol == base && p.quote_token.symbol == quote)
            .map(|p| p.price)
    }

    async fn get_liquidity(&self, base: &str, quote: &str) -> Option<Decimal> {
        self.pairs.iter()
            .find(|p| p.base_token.symbol == base && p.quote_token.symbol == quote)
            .map(|p| p.liquidity)
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        debug!("MockPriceFeed refresh called (no-op)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_price_feed() {
        let feed = MockPriceFeed::new();
        let pairs = feed.get_trading_pairs().await;

        assert!(!pairs.is_empty());
        assert!(pairs.iter().any(|p| p.base_token.symbol == "WETH"));
    }

    #[tokio::test]
    async fn test_mock_get_price() {
        let feed = MockPriceFeed::new();
        let price = feed.get_price("WETH", "USDC").await;

        assert!(price.is_some());
        assert!(price.unwrap() > Decimal::ZERO);
    }
}
