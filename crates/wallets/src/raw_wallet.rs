use crate::{utils, PendingSigner, WalletSigner};
use clap::Parser;
use eyre::Result;
use serde::Serialize;

/// A wrapper for the raw data options for `Wallet`, extracted to also be used standalone.
/// The raw wallet options can either be:
/// 1. Private Key (cleartext in CLI)
/// 2. Private Key (interactively via secure prompt)
/// 3. Mnemonic (via file path)
#[derive(Clone, Debug, Default, Serialize, Parser)]
#[clap(next_help_heading = "Wallet options - raw", about = None, long_about = None)]
pub struct RawWallet {
    /// Open an interactive prompt to enter your private key.
    #[clap(long, short)]
    pub interactive: bool,

    /// Use the provided private key.
    #[clap(long, value_name = "RAW_PRIVATE_KEY")]
    pub private_key: Option<String>,

    /// Use the mnemonic phrase of mnemonic file at the specified path.
    #[clap(long, alias = "mnemonic-path")]
    pub mnemonic: Option<String>,

    /// Use a BIP39 passphrase for the mnemonic.
    #[clap(long, value_name = "PASSPHRASE")]
    pub mnemonic_passphrase: Option<String>,

    /// The wallet derivation path.
    ///
    /// Works with both --mnemonic-path and hardware wallets.
    #[clap(long = "mnemonic-derivation-path", alias = "hd-path", value_name = "PATH")]
    pub hd_path: Option<String>,

    /// Use the private key from the given mnemonic index.
    ///
    /// Used with --mnemonic-path.
    #[clap(long, conflicts_with = "hd_path", default_value_t = 0, value_name = "INDEX")]
    pub mnemonic_index: u32,
}

impl RawWallet {
    pub fn signer(&self) -> Result<Option<WalletSigner>> {
        if self.interactive {
            return Ok(Some(PendingSigner::Interactive.unlock()?));
        }
        if let Some(private_key) = &self.private_key {
            return Ok(Some(utils::create_private_key_signer(private_key)?))
        }
        if let Some(mnemonic) = &self.mnemonic {
            return Ok(Some(utils::create_mnemonic_signer(
                mnemonic,
                self.mnemonic_passphrase.as_deref(),
                self.hd_path.as_deref(),
                self.mnemonic_index,
            )?))
        }
        Ok(None)
    }
}
