//! Furucombo Metis Arbitrage Engine
//!
//! A high-performance arbitrage scanner for Metis blockchain
//!
//! # Phase 1: Real Metis Price Feeds
//! - MetisPriceFeed implementation fetching from DEX Screener
//! - Support for Netswap and Tethys DEXes
//! - Caching layer for price data
//!
//! # Architecture
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                    Main Loop                         │
//! │  (Periodic scanning and opportunity detection)       │
//! └────────────────────────┬────────────────────────────┘
//!                          │
//!                          ▼
//! ┌─────────────────────────────────────────────────────┐
//! │                  PriceFeed Trait                     │
//! │  - get_trading_pairs()                               │
//! │  - get_price(base, quote)                            │
//! │  - refresh()                                         │
//! └────────────────────────┬────────────────────────────┘
//!                          │
//!          ┌───────────────┴───────────────┐
//!          │                               │
//!          ▼                               ▼
//! ┌─────────────────┐           ┌─────────────────┐
//! │  MetisPriceFeed │           │  MockPriceFeed  │
//! │  (Production)   │           │  (Testing)      │
//! └─────────────────┘           └─────────────────┘
//! ```

pub mod models;
pub mod price_feed;

// Re-export commonly used types
pub use models::{
    ArbitrageLeg,
    ArbitrageOpportunity,
    ArbitrageRoute,
    CachedPrice,
    Exchange,
    Token,
    TradingPair,
};

pub use price_feed::{MetisPriceFeed, MockPriceFeed, PriceFeed};

/// Version of the arbitrage engine
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Name of the package
pub const NAME: &str = env!("CARGO_PKG_NAME");
