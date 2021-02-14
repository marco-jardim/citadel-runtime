// MyCitadel: node, wallet library & command-line tool
// Written in 2021 by
//     Dr. Maxim Orlovsky <orlovsky@mycitadel.io>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the AGPL License
// along with this software.
// If not, see <https://www.gnu.org/licenses/agpl-3.0-standalone.html>.

use clap::{AppSettings, ArgGroup, Clap, ValueHint};
use invoice::Invoice;
use std::str::FromStr;
use wallet::bip32::PubkeyChain;
use wallet::descriptor;

use crate::model;

pub const MYCITADEL_CLI_CONFIG: &'static str = "{data_dir}/mycitadel-cli.toml";

#[derive(
    Clap, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display,
)]
pub enum Formatting {
    /// Print only data identifier strings (in Bech32m format), one per line
    #[display("id")]
    Id,

    /// Print a single entry per line formatted with a compact formatting
    /// option (type-specifc). This can be, for instance, `<txid>:<vout>`
    /// format for transaction outpoint, etc.
    #[display("compact")]
    Compact,

    /// Print tab-separated list of items
    #[display("tab")]
    Tab,

    /// Print comma-separated list of items
    #[display("csv")]
    Csv,

    /// Output data as formatted YAML
    #[display("yaml")]
    Yaml,

    /// Output data as JSON
    #[display("json")]
    Json,
}

impl FromStr for Formatting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "id" => Formatting::Id,
            "compact" => Formatting::Compact,
            "tab" => Formatting::Tab,
            "csv" => Formatting::Csv,
            "yaml" => Formatting::Yaml,
            "json" => Formatting::Json,
            _ => Err("Unknown format name")?,
        })
    }
}

#[derive(Clap, Clone, Debug)]
#[clap(
    name = "mycitadel-cli",
    bin_name = "mycitadel-cli",
    author,
    version,
    about = "Command-line tool for working with MyCitadel node",
    setting = AppSettings::ColoredHelp,
    group = ArgGroup::new("descriptor").required(false)
)]
pub struct Opts {
    /// These params can be read also from the configuration file, not just
    /// command-line args or environment variables
    #[clap(flatten)]
    pub shared: crate::opts::SharedOpts,

    /// Path to the configuration file.
    ///
    /// NB: Command-line options override configuration file values.
    #[clap(
        short,
        long,
        default_value = MYCITADEL_CLI_CONFIG,
        env = "MYCITADEL_CLI_CONFIG",
        value_hint = ValueHint::FilePath
    )]
    pub config: String,

    /// Command to execute
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum Command {
    /// Wallet management commands
    #[display("wallet {subcommand}")]
    Wallet {
        #[clap(subcommand)]
        subcommand: WalletCommand,
    },

    /// Address-related commands
    #[display("address")]
    Address {
        #[clap(subcommand)]
        subcommand: AddressCommand,
    },

    /// Asset management commands
    #[display("asset {subcommand}")]
    Asset {
        #[clap(subcommand)]
        subcommand: AssetCommand,
    },

    /// Invoice-related commands
    #[display("invoice {subcommand}")]
    Invoice {
        #[clap(subcommand)]
        subcommand: InvoiceCommand,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum WalletCommand {
    /// Lists existing wallets
    #[display("list")]
    List {
        /// How the wallet list should be formatted
        #[clap(short, long, default_value = "tab", global = true)]
        format: Formatting,
    },

    /// Creates wallet with a given name and descriptor parameters
    #[display("create {subcommand}")]
    Create {
        #[clap(subcommand)]
        subcommand: WalletCreateCommand,
    },

    /// Change a name of a wallet
    #[display("rename {wallet_id} \"{new_name}\"")]
    Rename {
        /// Wallet id to rename
        #[clap()]
        wallet_id: model::ContractId,

        /// New name of the wallet
        #[clap()]
        new_name: String,
    },

    /// Delete existing wallet contract
    #[display("delete {wallet_id}")]
    Delete {
        /// Wallet id to delete
        #[clap()]
        wallet_id: model::ContractId,
    },

    /// Returns detailed wallet balance information
    Balance {
        #[clap(flatten)]
        scan_opts: WalletOpts,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum WalletCreateCommand {
    /// Creates current single-sig wallet account
    #[display("single-sig {name} {pubkey_chain}")]
    SingleSig {
        /// Wallet name
        #[clap()]
        name: String,

        /// Extended public key with derivation info.
        ///
        /// It should be a BIP32 derivation string which provides an extended
        /// public key value at the level after which no hardened
        /// derives is used. For instance,
        /// `m/84'/0'=[xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8]/*`,
        /// or, simply
        /// `[xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8]/*`,
        /// if you dont want your wallet to keep public key source information.
        ///
        /// You can use more advanced scenarios allowing full record of the
        /// key origin and extending derivation paths with range values:
        /// `![6734cda8]/84'/0'/1'
        /// =[xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8]/
        /// 0-1/*`
        #[clap()]
        pubkey_chain: PubkeyChain,

        #[clap(flatten)]
        opts: DescriptorOpts,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum AddressCommand {
    /// Print address list
    ListUsed {
        #[clap(flatten)]
        scan_opts: WalletOpts,

        /// Limit the number of addresses printed
        #[clap(short, long, global = true)]
        limit: Option<usize>,
    },

    Create {
        /// Create address at custom index number
        #[clap(short, long)]
        index: Option<u32>,

        /// Whether to mark address as used
        #[clap(short = 'u', long = "unmarked", global = true, parse(from_flag = std::ops::Not::not))]
        mark_used: bool,

        /// Number of addresses to create
        #[clap(short, long, default_value = "1")]
        no: u8,

        /// Use SegWit legacy address format (applicable only to a SegWit
        /// wallets)
        #[clap(long, takes_value = false, global = true)]
        legacy: bool,
    },

    MarkUsed {
        /// Index of address derivation path (use `address list` command to see
        /// address indexes
        index: Option<u32>,

        /// Use SegWit legacy address format (applicable only to a SegWit
        /// wallets)
        #[clap(long, takes_value = false, global = true)]
        legacy: bool,

        /// Remove use mark (inverses the command)
        #[clap(short, long, takes_value = false, global = true)]
        unmark: bool,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum AssetCommand {
    /// Lists known assets
    #[display("list")]
    List {
        /// How the asset list output should be formatted
        #[clap(short, long, default_value = "tab", global = true)]
        format: Formatting,
    },

    /// Import asset genesis data
    #[display("import")]
    Import {
        /// Bech32-representation of the asset genesis (string starting with
        /// `genesis1....`
        #[clap()]
        genesis: String,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[clap(setting = AppSettings::ColoredHelp)]
pub enum InvoiceCommand {
    /// Create new invoice
    #[display("create {amount} {beneficiary} ...")]
    Create {
        /// Asset in which the payment is requested; defaults to bitcoin
        #[clap(short, long = "asset")]
        asset_id: Option<rgb::ContractId>,

        /// Amount of the asset (in the smallest asset units, without floating
        /// point - i.e. for bitcoin use satoshis)
        #[clap()]
        amount: rgb::AtomicValue,

        /// Wallet id to which the payment should be received
        #[clap()]
        beneficiary: model::ContractId,

        /// Optional details about the merchant providing the invoice
        #[clap(short, long)]
        merchant: Option<String>,

        /// Information about the invoice
        #[clap(short, long)]
        details: Option<String>,

        /// Do not mark the address used in the invoice as used
        #[clap(short, long)]
        unmark: bool,

        /// Create descriptor-based invoice (not compatible with instant wallet
        /// accounts)
        #[clap(long = "descriptor")]
        descriptor_based: bool,

        /// Create a PSBT-based invoice (not compatible with instant wallet
        /// accounts)
        #[clap(long = "psbt")]
        psbt_based: bool,
    },

    /// List all known invoices
    List {
        /// How invoice list should be formatted
        #[clap(short, long, default_value = "tab", global = true)]
        format: Formatting,
    },

    /// Parse invoice and print out its detailed information
    Info {
        /// Invoice Bech32 string representation
        #[clap()]
        invoice: Invoice,

        /// Format to use for the invoice representation
        #[clap(short, long, default_value = "yaml", global = true)]
        format: Formatting,
    },

    /// Pay an invoice
    Pay {
        /// Invoice Bech32 string representation
        #[clap()]
        invoice: Invoice,

        /// Wallet to pay from
        #[clap()]
        wallet_id: model::ContractId,
    },

    /// Accept payment for the invoice. Required only for on-chain RGB
    /// payments; Bitcoin & Lightning-network payments (including RGB
    /// lightning) are accepted automatically and does not require calling
    /// this method.
    Accept {
        /// Your original invoice for acceptance
        #[clap()]
        invoice: Invoice,

        /// Consignment data to accept
        #[clap()]
        consignment: String,

        /// Whether parameter given by consignment is a file name or a Bech32
        /// string
        #[clap(short, long)]
        file: bool,
    },
}

#[derive(Clap, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WalletOpts {
    /// Wallet id for the operation
    #[clap()]
    pub wallet_id: model::ContractId,

    /// Whether to re-scan addresses space with Electrum server
    #[clap(short, long)]
    pub rescan: bool,

    /// How many addresses should be scanned at least after the final address
    /// with no transactions is reached. Defaults to 20
    #[clap(long, requires = "rescan", global = true)]
    pub lookup_depth: Option<u8>,

    /// How the command output should be formatted
    #[clap(short, long, default_value = "tab", global = true)]
    pub format: Formatting,
}

#[derive(Clap, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DescriptorOpts {
    /// Creates old "bare" wallets, where public key is kept in the
    /// explicit form within bitcoin transaction P2PK output
    #[clap(long, takes_value = false, conflicts_with_all = &["legacy", "segwit", "taproot"], global = true)]
    bare: bool,

    /// Whether create a pre-SegWit wallet (P2PKH) rather than SegWit
    /// (P2WPKH). If you'd like to use legacy SegWit-style addresses
    /// (P2WPKH-in-P2SH), do not use this flag, create normal
    /// SegWit wallet instead and specify `--legacy` option when
    /// requesting new address
    #[clap(long, takes_value = false, conflicts_with_all = &["bare", "segwit", "taproot"], global = true)]
    legacy: bool,

    /// Recommended SegWit wallet with P2WKH and P2WPKH-in-P2SH outputs
    #[clap(long, takes_value = false, conflicts_with_all = &["legacy", "bare", "taproot"], global = true)]
    segwit: bool,

    /// Reserved for the future taproot P2TR outputs
    #[clap(long, takes_value = false, conflicts_with_all = &["legacy", "segwit", "bare"], global = true)]
    taproot: bool,
}

impl DescriptorOpts {
    pub fn descriptor_category(self) -> descriptor::OuterCategory {
        if self.bare {
            descriptor::OuterCategory::Bare
        } else if self.legacy {
            descriptor::OuterCategory::Hashed
        } else if self.taproot {
            descriptor::OuterCategory::Taproot
        } else {
            descriptor::OuterCategory::SegWit
        }
    }
}
