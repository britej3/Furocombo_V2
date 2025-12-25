//! models.rs - Core data structures for the arbitrage engine
//!
//! Phase 1: Defines Token, Exchange, TradingPair and related types

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a token on the blockchain
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Token {
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub address: String,
}

impl Token {
    pub fn new(symbol: &str, name: &str, decimals: u8, address: &str) -> Self {
        Token {
            symbol: symbol.to_string(),
            name: name.to_string(),
            decimals,
            address: address.to_string(),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol)
    }
}

/// Represents a decentralized exchange
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Exchange {
    pub name: String,
    pub chain: String,
    pub router_address: String,
}

impl Exchange {
    pub fn new(name: &str, chain: &str, router_address: &str) -> Self {
        Exchange {
            name: name.to_string(),
            chain: chain.to_string(),
            router_address: router_address.to_string(),
        }
    }
}

impl fmt::Display for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.chain)
    }
}

/// Represents a trading pair on a DEX
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPair {
    pub base_token: Token,
    pub quote_token: Token,
    pub exchange: Exchange,
    pub price: Decimal,
    pub liquidity: Decimal,
    pub reserve_base: Decimal,
    pub reserve_quote: Decimal,
}

impl TradingPair {
    pub fn new(
        base_token: Token,
        quote_token: Token,
        exchange: Exchange,
        price: Decimal,
        liquidity: Decimal,
        reserve_base: Decimal,
        reserve_quote: Decimal,
    ) -> Self {
        TradingPair {
            base_token,
            quote_token,
            exchange,
            price,
            liquidity,
            reserve_base,
            reserve_quote,
        }
    }

    /// Returns the pair identifier (e.g., "WETH/USDC")
    pub fn pair_id(&self) -> String {
        format!("{}/{}", self.base_token.symbol, self.quote_token.symbol)
    }

    /// Returns full identifier including exchange
    pub fn full_id(&self) -> String {
        format!("{}:{}/{}",
            self.exchange.name,
            self.base_token.symbol,
            self.quote_token.symbol
        )
    }
}

impl fmt::Display for TradingPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{} on {} @ {}",
            self.base_token.symbol,
            self.quote_token.symbol,
            self.exchange.name,
            self.price
        )
    }
}

/// Cached price entry with timestamp
#[derive(Debug, Clone)]
pub struct CachedPrice {
    pub price: Decimal,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: String,
}

impl CachedPrice {
    /// Check if the cached price is stale (older than max_age_seconds)
    pub fn is_stale(&self, max_age_seconds: i64) -> bool {
        let age = chrono::Utc::now().signed_duration_since(self.timestamp);
        age.num_seconds() > max_age_seconds
    }
}

/// Represents a single leg of an arbitrage route
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageLeg {
    pub from_token: Token,
    pub to_token: Token,
    pub exchange: Exchange,
    pub price: Decimal,
    pub liquidity: Decimal,
}

impl ArbitrageLeg {
    pub fn new(
        from_token: Token,
        to_token: Token,
        exchange: Exchange,
        price: Decimal,
        liquidity: Decimal,
    ) -> Self {
        ArbitrageLeg {
            from_token,
            to_token,
            exchange,
            price,
            liquidity,
        }
    }
}

/// Represents a complete arbitrage route (sequence of trades)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageRoute {
    pub legs: Vec<ArbitrageLeg>,
    pub total_hops: usize,
}

impl ArbitrageRoute {
    pub fn new(legs: Vec<ArbitrageLeg>) -> Self {
        let total_hops = legs.len();
        ArbitrageRoute { legs, total_hops }
    }

    /// Format the route as a string (e.g., "USDC -> WETH -> METIS -> USDC")
    pub fn format_path(&self) -> String {
        if self.legs.is_empty() {
            return String::new();
        }

        let mut path = vec![self.legs[0].from_token.symbol.clone()];
        for leg in &self.legs {
            path.push(leg.to_token.symbol.clone());
        }
        path.join(" -> ")
    }
}

/// Represents a detected arbitrage opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub route: ArbitrageRoute,
    pub input_amount: Decimal,
    pub output_amount: Decimal,
    pub gross_profit: Decimal,
    pub net_profit: Decimal,
    pub gas_cost: Decimal,
    pub profit_percentage: Decimal,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ArbitrageOpportunity {
    pub fn new(
        route: ArbitrageRoute,
        input_amount: Decimal,
        output_amount: Decimal,
        gross_profit: Decimal,
        net_profit: Decimal,
        gas_cost: Decimal,
    ) -> Self {
        let profit_percentage = if input_amount > Decimal::ZERO {
            (net_profit / input_amount) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        ArbitrageOpportunity {
            route,
            input_amount,
            output_amount,
            gross_profit,
            net_profit,
            gas_cost,
            profit_percentage,
            timestamp: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_token_creation() {
        let token = Token::new("WETH", "Wrapped Ether", 18, "0x123...");
        assert_eq!(token.symbol, "WETH");
        assert_eq!(token.decimals, 18);
    }

    #[test]
    fn test_trading_pair_id() {
        let base = Token::new("WETH", "Wrapped Ether", 18, "0x123");
        let quote = Token::new("USDC", "USD Coin", 6, "0x456");
        let exchange = Exchange::new("netswap", "Metis", "0x789");

        let pair = TradingPair::new(
            base, quote, exchange,
            dec!(1800.50),
            dec!(500000),
            dec!(100),
            dec!(180050),
        );

        assert_eq!(pair.pair_id(), "WETH/USDC");
        assert_eq!(pair.full_id(), "netswap:WETH/USDC");
    }

    #[test]
    fn test_cached_price_staleness() {
        let cached = CachedPrice {
            price: dec!(1800),
            timestamp: chrono::Utc::now() - chrono::Duration::seconds(120),
            source: "test".to_string(),
        };

        assert!(cached.is_stale(60));  // 120s old > 60s max
        assert!(!cached.is_stale(180)); // 120s old < 180s max
    }
}
