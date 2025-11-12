use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ethers::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

// Compound V2 cToken ABI methods
abigen!(
    CToken,
    r#"[
        function getCash() external view returns (uint256)
        function totalBorrows() external view returns (uint256)
        function totalReserves() external view returns (uint256)
        function symbol() external view returns (string)
    ]"#,
);

// Compound V3 Comet ABI methods
abigen!(
    Comet,
    r#"[
        function getReserves() external view returns (int256)
        function totalSupply() external view returns (uint256)
        function totalBorrow() external view returns (uint256)
        function balanceOf(address account) external view returns (uint256)
        function getUtilization() external view returns (uint256)
        function baseToken() external view returns (address)
        function supply(address asset, uint256 amount) external
        function withdraw(address asset, uint256 amount) external
        function getSupplyRate(uint256 utilization) external view returns (uint64)
        function getBorrowRate(uint256 utilization) external view returns (uint64)
    ]"#,
);

// ERC20 token interface
abigen!(
    ERC20,
    r#"[
        function balanceOf(address account) external view returns (uint256)
        function approve(address spender, uint256 amount) external returns (bool)
        function allowance(address owner, address spender) external view returns (uint256)
        function symbol() external view returns (string)
        function decimals() external view returns (uint8)
    ]"#,
);

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum CompoundVersion {
    V2,
    V3,
}

impl Default for CompoundVersion {
    fn default() -> Self {
        CompoundVersion::V2
    }
}

#[derive(Parser, Debug)]
#[command(name = "compound-monitor")]
#[command(about = "Monitor and interact with Compound Finance markets", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Monitor liquidity (default mode)
    Monitor,
    /// Deposit (supply) assets to Compound
    Supply {
        /// Amount to supply (in base units, e.g., 1000000 = 1 USDC)
        #[arg(short, long)]
        amount: String,
        /// Private key for signing transactions (optional if set in config.json)
        #[arg(short, long)]
        private_key: Option<String>,
    },
    /// Withdraw assets from Compound
    Withdraw {
        /// Amount to withdraw (in base units, e.g., 1000000 = 1 USDC)
        #[arg(short, long)]
        amount: String,
        /// Private key for signing transactions (optional if set in config.json)
        #[arg(short, long)]
        private_key: Option<String>,
    },
    /// Check your balance
    Balance {
        /// Wallet address to check (optional if using monitor_address.json)
        #[arg(short, long)]
        address: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    #[serde(default)]
    compound_version: CompoundVersion,
    rpc_url: String,
    market_address: String,
    market_name: Option<String>,
    webhook_url: String,
    poll_interval_secs: u64,
    liquidity_threshold: String,
    notification_enabled: Option<bool>,
    /// Optional private key for transactions (keep this secure!)
    private_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MonitorAddress {
    name: String,
    address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MonitorAddressConfig {
    addresses: Vec<MonitorAddress>,
}

impl MonitorAddressConfig {
    fn load() -> Result<Self> {
        let config_path = "monitor_address.json";

        let config_content = fs::read_to_string(config_path)
            .context("Failed to read monitor_address.json. Make sure it exists in the current directory.")?;

        let config: MonitorAddressConfig = serde_json::from_str(&config_content)
            .context("Failed to parse monitor_address.json. Check JSON syntax.")?;

        Ok(config)
    }
}

#[derive(Debug, Serialize)]
struct LiquidityAlert {
    market_address: String,
    market_symbol: String,
    available_liquidity: String,
    total_borrows: String,
    total_reserves: String,
    threshold: String,
    timestamp: i64,
    message: String,
}

impl Config {
    fn load() -> Result<Self> {
        let config_path = "config.json";

        info!("Loading configuration from {}", config_path);

        let config_content = fs::read_to_string(config_path)
            .context("Failed to read config.json. Make sure it exists in the current directory.")?;

        let config: Config = serde_json::from_str(&config_content)
            .context("Failed to parse config.json. Check JSON syntax.")?;

        Ok(config)
    }
}

struct CompoundMonitor {
    config: Config,
    provider: Arc<Provider<Http>>,
    client: reqwest::Client,
    threshold: U256,
}

impl CompoundMonitor {
    async fn new(config: Config) -> Result<Self> {
        let provider = Provider::<Http>::try_from(&config.rpc_url)
            .context("Failed to create provider")?;
        let provider = Arc::new(provider);

        let client = reqwest::Client::new();

        let threshold = U256::from_dec_str(&config.liquidity_threshold)
            .context("Invalid liquidity threshold")?;

        Ok(Self {
            config,
            provider,
            client,
            threshold,
        })
    }

    async fn check_liquidity(&self) -> Result<(U256, U256, U256, String)> {
        let address: H160 = self.config.market_address.parse()
            .context("Invalid market address")?;

        match self.config.compound_version {
            CompoundVersion::V2 => self.check_liquidity_v2(address).await,
            CompoundVersion::V3 => self.check_liquidity_v3(address).await,
        }
    }

    async fn check_liquidity_v2(&self, address: H160) -> Result<(U256, U256, U256, String)> {
        let contract = CToken::new(address, Arc::clone(&self.provider));

        // Get available cash (liquidity)
        let cash = contract.get_cash().call().await
            .context("Failed to get cash (V2)")?;

        // Get total borrows
        let borrows = contract.total_borrows().call().await
            .context("Failed to get total borrows (V2)")?;

        // Get total reserves
        let reserves = contract.total_reserves().call().await
            .context("Failed to get total reserves (V2)")?;

        // Get market symbol
        let symbol = contract.symbol().call().await
            .context("Failed to get symbol (V2)")?;

        info!(
            "Market: {} | Available Liquidity: {} | Borrows: {} | Reserves: {}",
            symbol, cash, borrows, reserves
        );

        Ok((cash, borrows, reserves, symbol))
    }

    async fn check_liquidity_v3(&self, address: H160) -> Result<(U256, U256, U256, String)> {
        let contract = Comet::new(address, Arc::clone(&self.provider));

        // Get the base token address (e.g., USDC)
        let base_token_address = contract.base_token().call().await
            .context("Failed to get base token address (V3)")?;

        // Get the actual balance of base token held by the Comet contract
        let base_token = ERC20::new(base_token_address, Arc::clone(&self.provider));
        let contract_balance = base_token.balance_of(address).call().await
            .context("Failed to get contract balance (V3)")?;

        // Get total supply (total assets supplied to the protocol)
        let total_supply = contract.total_supply().call().await
            .context("Failed to get total supply (V3)")?;

        // Get total borrows
        let total_borrow = contract.total_borrow().call().await
            .context("Failed to get total borrow (V3)")?;

        // Get reserves (can be negative in V3)
        let reserves_i256 = contract.get_reserves().call().await
            .context("Failed to get reserves (V3)")?;

        // Convert I256 to U256 (take absolute value if negative)
        let reserves = if reserves_i256 >= I256::zero() {
            U256::from(reserves_i256.as_u128())
        } else {
            U256::zero()
        };

        // Available liquidity is the actual balance of base token in the contract
        let available_liquidity = contract_balance;

        // Get utilization for APY calculation
        let utilization = contract.get_utilization().call().await
            .context("Failed to get utilization (V3)")?;

        // Get supply and borrow rates
        let supply_rate = contract.get_supply_rate(utilization).call().await
            .context("Failed to get supply rate (V3)")?;
        let borrow_rate = contract.get_borrow_rate(utilization).call().await
            .context("Failed to get borrow rate (V3)")?;

        // Calculate APY from rates
        // Rates are per second with 18 decimals (1e18 = 100% per second)
        let supply_apy = self.calculate_apy(supply_rate);
        let borrow_apy = self.calculate_apy(borrow_rate);

        let symbol = self.config.market_name.clone()
            .unwrap_or_else(|| "cUSDCv3".to_string());

        info!(
            "Market: {} | Available Liquidity: {} | Total Supply: {} | Total Borrow: {} | Reserves: {}",
            symbol, available_liquidity, total_supply, total_borrow, reserves
        );
        // Convert utilization to percentage (utilization is scaled by 1e18)
        let utilization_pct = utilization.as_u128() as f64 / 1e16;

        info!(
            "Supply APY: {:.2}% | Borrow APY: {:.2}% | Utilization: {:.2}%",
            supply_apy, borrow_apy, utilization_pct
        );

        Ok((available_liquidity, total_borrow, reserves, symbol))
    }

    async fn send_alert(&self, alert: LiquidityAlert) -> Result<()> {
        info!("Sending alert to webhook: {}", self.config.webhook_url);

        let response = self.client
            .post(&self.config.webhook_url)
            .json(&alert)
            .send()
            .await
            .context("Failed to send webhook request")?;

        if response.status().is_success() {
            info!("Alert sent successfully");
        } else {
            warn!("Alert sent but received non-success status: {}", response.status());
        }

        Ok(())
    }

    async fn supply_v3(&self, amount: U256, private_key: &str) -> Result<()> {
        info!("Supplying {} to Compound V3...", amount);

        let wallet = private_key.parse::<LocalWallet>()
            .context("Invalid private key")?;
        let wallet = wallet.with_chain_id(1u64); // Mainnet

        let provider = Provider::<Http>::try_from(&self.config.rpc_url)?;
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);

        let market_address: H160 = self.config.market_address.parse()?;
        let contract = Comet::new(market_address, client.clone());

        // Get base token address
        let base_token_address = contract.base_token().call().await?;
        let base_token = ERC20::new(base_token_address, client.clone());

        // Check allowance
        let allowance = base_token.allowance(client.address(), market_address).call().await?;

        if allowance < amount {
            info!("Approving Compound to spend tokens...");
            let approve_tx = base_token.approve(market_address, U256::MAX);
            let pending_tx = approve_tx.send().await?;
            let receipt = pending_tx.await?.context("Approve transaction failed")?;
            info!("Approved! Transaction hash: {:?}", receipt.transaction_hash);
        }

        // Supply to Compound
        info!("Sending supply transaction...");
        let supply_tx = contract.supply(base_token_address, amount);
        let pending_tx = supply_tx.send().await?;
        let receipt = pending_tx.await?.context("Supply transaction failed")?;

        info!("✓ Supply successful!");
        info!("Transaction hash: {:?}", receipt.transaction_hash);
        info!("Gas used: {:?}", receipt.gas_used);

        Ok(())
    }

    async fn withdraw_v3(&self, amount: U256, private_key: &str) -> Result<()> {
        info!("Withdrawing {} from Compound V3...", amount);

        let wallet = private_key.parse::<LocalWallet>()
            .context("Invalid private key")?;
        let wallet = wallet.with_chain_id(1u64); // Mainnet

        let provider = Provider::<Http>::try_from(&self.config.rpc_url)?;
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);

        let market_address: H160 = self.config.market_address.parse()?;
        let contract = Comet::new(market_address, client);

        // Get base token address
        let base_token_address = contract.base_token().call().await?;

        // Withdraw from Compound
        info!("Sending withdraw transaction...");
        let withdraw_tx = contract.withdraw(base_token_address, amount);
        let pending_tx = withdraw_tx.send().await?;
        let receipt = pending_tx.await?.context("Withdraw transaction failed")?;

        info!("✓ Withdraw successful!");
        info!("Transaction hash: {:?}", receipt.transaction_hash);
        info!("Gas used: {:?}", receipt.gas_used);

        Ok(())
    }

    async fn check_balance(&self, address: &str, name: Option<&str>) -> Result<()> {
        let address: H160 = address.parse().context("Invalid address")?;
        let market_address: H160 = self.config.market_address.parse()?;

        let contract = Comet::new(market_address, Arc::clone(&self.provider));

        // Get base token address
        let base_token_address = contract.base_token().call().await?;
        let base_token = ERC20::new(base_token_address, Arc::clone(&self.provider));

        // Get token info
        let symbol = base_token.symbol().call().await?;
        let decimals = base_token.decimals().call().await?;

        // Check wallet balance
        let wallet_balance = base_token.balance_of(address).call().await?;

        // Check Compound balance
        let compound_balance = contract.balance_of(address).call().await?;

        // Format balances for display
        let divisor = U256::from(10u128.pow(decimals as u32));
        let wallet_formatted = self.format_balance(wallet_balance, divisor);
        let compound_formatted = self.format_balance(compound_balance, divisor);

        info!("═══════════════════════════════════════════════════");
        if let Some(name) = name {
            info!("Name: {}", name);
        }
        info!("Address: {}", address);
        info!("Token: {} (base token: {})", symbol, base_token_address);
        info!("Decimals: {}", decimals);
        info!("───────────────────────────────────────────────────");
        info!("Wallet balance:   {} {} ({})", wallet_formatted, symbol, wallet_balance);
        info!("Compound balance: {} {} ({})", compound_formatted, symbol, compound_balance);
        info!("═══════════════════════════════════════════════════");

        Ok(())
    }

    async fn check_balance_batch(&self) -> Result<()> {
        let address_config = MonitorAddressConfig::load()?;

        if address_config.addresses.is_empty() {
            info!("No addresses found in monitor_address.json");
            return Ok(());
        }

        info!("Checking balances for {} addresses...", address_config.addresses.len());
        info!("");

        for monitor_addr in &address_config.addresses {
            match self.check_balance(&monitor_addr.address, Some(&monitor_addr.name)).await {
                Ok(_) => info!(""),
                Err(e) => {
                    error!("Failed to check balance for {} ({}): {}",
                        monitor_addr.name, monitor_addr.address, e);
                    info!("");
                }
            }
        }

        Ok(())
    }

    fn format_balance(&self, balance: U256, divisor: U256) -> String {
        if divisor.is_zero() {
            return balance.to_string();
        }

        let whole = balance / divisor;
        let remainder = balance % divisor;

        if remainder.is_zero() {
            format!("{}", whole)
        } else {
            // Calculate decimal part
            let decimals_str = format!("{:0width$}", remainder, width = divisor.to_string().len() - 1);
            let trimmed = decimals_str.trim_end_matches('0');
            if trimmed.is_empty() {
                format!("{}", whole)
            } else {
                format!("{}.{}", whole, trimmed)
            }
        }
    }

    fn calculate_apy(&self, rate_per_second: u64) -> f64 {
        // Compound V3 rates are per second with scaling factor
        // rate_per_second is in units where 1e18 = 100% per second
        // APY = (1 + rate_per_second)^(seconds_per_year) - 1

        const SECONDS_PER_YEAR: f64 = 365.25 * 24.0 * 60.0 * 60.0; // 31,557,600
        const SCALE: f64 = 1e18;

        let rate = rate_per_second as f64 / SCALE;
        let apy = ((1.0 + rate).powf(SECONDS_PER_YEAR) - 1.0) * 100.0;

        apy
    }

    async fn run(&self) -> Result<()> {
        let version_str = match self.config.compound_version {
            CompoundVersion::V2 => "V2",
            CompoundVersion::V3 => "V3 (Comet)",
        };

        info!("Starting Compound {} liquidity monitor...", version_str);
        if let Some(ref name) = self.config.market_name {
            info!("Market: {} ({})", name, self.config.market_address);
        } else {
            info!("Market: {}", self.config.market_address);
        }
        info!("Threshold: {}", self.config.liquidity_threshold);
        info!("Poll interval: {}s", self.config.poll_interval_secs);
        info!("Notifications: {}", if self.config.notification_enabled.unwrap_or(true) { "enabled" } else { "disabled" });

        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.poll_interval_secs)
        );

        loop {
            interval.tick().await;

            match self.check_liquidity().await {
                Ok((liquidity, borrows, reserves, symbol)) => {
                    if liquidity < self.threshold {
                        warn!(
                            "Liquidity below threshold! Current: {}, Threshold: {}",
                            liquidity, self.threshold
                        );

                        let alert = LiquidityAlert {
                            market_address: self.config.market_address.clone(),
                            market_symbol: symbol,
                            available_liquidity: liquidity.to_string(),
                            total_borrows: borrows.to_string(),
                            total_reserves: reserves.to_string(),
                            threshold: self.threshold.to_string(),
                            timestamp: chrono::Utc::now().timestamp(),
                            message: format!(
                                "Available liquidity ({}) is below threshold ({})",
                                liquidity, self.threshold
                            ),
                        };

                        // Only send alert if notifications are enabled
                        if self.config.notification_enabled.unwrap_or(true) {
                            if let Err(e) = self.send_alert(alert).await {
                                error!("Failed to send alert: {}", e);
                            }
                        } else {
                            info!("Notification disabled, skipping alert");
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to check liquidity: {}", e);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    let cli = Cli::parse();

    let config = Config::load()
        .context("Failed to load configuration")?;

    let monitor = CompoundMonitor::new(config.clone()).await?;

    match cli.command {
        Some(Commands::Supply { amount, private_key }) => {
            let amount = U256::from_dec_str(&amount)
                .context("Invalid amount")?;

            if monitor.config.compound_version != CompoundVersion::V3 {
                anyhow::bail!("Supply/withdraw is only supported for Compound V3. Set 'compound_version': 'v3' in config.json");
            }

            // Use CLI private key if provided, otherwise use config
            let key = private_key
                .or_else(|| monitor.config.private_key.clone())
                .context("Private key not provided. Use --private-key or add 'private_key' to config.json")?;

            monitor.supply_v3(amount, &key).await?;
        }
        Some(Commands::Withdraw { amount, private_key }) => {
            let amount = U256::from_dec_str(&amount)
                .context("Invalid amount")?;

            if monitor.config.compound_version != CompoundVersion::V3 {
                anyhow::bail!("Supply/withdraw is only supported for Compound V3. Set 'compound_version': 'v3' in config.json");
            }

            // Use CLI private key if provided, otherwise use config
            let key = private_key
                .or_else(|| monitor.config.private_key.clone())
                .context("Private key not provided. Use --private-key or add 'private_key' to config.json")?;

            monitor.withdraw_v3(amount, &key).await?;
        }
        Some(Commands::Balance { address }) => {
            if let Some(addr) = address {
                // Check single address from command line
                monitor.check_balance(&addr, None).await?;
            } else {
                // Check all addresses from monitor_address.json
                monitor.check_balance_batch().await?;
            }
        }
        Some(Commands::Monitor) | None => {
            // Default: run monitor
            monitor.run().await?;
        }
    }

    Ok(())
}
