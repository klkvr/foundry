use alloy_primitives::{keccak256, Address};
use clap::{
    builder::{PossibleValuesParser, TypedValueParser},
    Parser,
};
use ethers::{
    core::{k256::ecdsa::SigningKey, rand::thread_rng},
    signers::coins_bip39::{English, Mnemonic},
    utils::secret_key_to_address,
};
use ethers_core::rand::Rng;
use eyre::Result;
use foundry_utils::types::ToAlloy;
use rayon::{iter, iter::ParallelIterator};
use regex::Regex;
use std::time::Instant;

#[derive(Debug, Parser)]
pub enum VanitySubcommands {
    /// Generate a private key for address by given prefix and/or suffix.
    #[clap(visible_alias = "pk")]
    PrivateKey {
        #[clap(flatten)]
        args: VanityOpts,
    },
    /// Generate a mnemonic for address by given prefix and/or suffix.
    #[clap(visible_aliases = &["m", "mnemo"])]
    Mnemonic {
        /// Number of words in the mnemonic
        #[clap(long, default_value = "12", value_parser = PossibleValuesParser::new(&[
            "12", "15", "18", "21", "24"
        ]).map(|s| s.parse::<usize>().unwrap()))]
        words: usize,

        #[clap(flatten)]
        args: VanityOpts,
    },
    /// Generate CREATE2 salt for address by given prefix and/or suffix.
    #[clap(visible_aliases = &["salt", "c2"])]
    Create2 {
        /// Address of the contract deployer
        #[clap(
            short,
            long,
            default_value = "0x4e59b44847b379578588920ca78fbf26c0b4956c",
            value_name = "ADDRESS"
        )]
        deployer: Address,

        /// Init code of the contract to be deployed.
        #[clap(short, long, value_name = "HEX")]
        init_code: Option<String>,

        /// Init code hash of the contract to be deployed.
        #[clap(alias = "ch", long, value_name = "HASH")]
        init_code_hash: Option<String>,

        #[clap(flatten)]
        args: VanityOpts,
    },
}

impl VanitySubcommands {
    pub fn run(self) -> Result<Address> {
        let timer = Instant::now();

        let addr = match self {
            Self::PrivateKey { args } => {
                let generator = PrivateKeyGenerator {};

                let (params, addr) = args.find_vanity(generator)?;

                println!("Private key: 0x{}", hex::encode(params.key.to_bytes()));

                addr
            }
            Self::Mnemonic { args, words } => {
                let generator = MnemonicGenerator { words };

                let (params, addr) = args.find_vanity(generator)?;

                println!("Mnemonic: {}", params.mnemonic.to_phrase());

                addr
            }
            Self::Create2 { args, deployer, init_code, init_code_hash } => {
                if init_code.is_none() && init_code_hash.is_none() {
                    eyre::bail!("You must provide init code or init code hash");
                }

                let init_code_hash = if let Some(init_code_hash) = init_code_hash {
                    let mut a: [u8; 32] = [0; 32];
                    let init_code_hash_bytes = hex::decode(init_code_hash)?;
                    assert!(
                        init_code_hash_bytes.len() == 32,
                        "init code hash should be 32 bytes long"
                    );
                    a.copy_from_slice(&init_code_hash_bytes);
                    a.into()
                } else {
                    keccak256(hex::decode(init_code.unwrap())?)
                };

                let generator =
                    Create2Generator { deployer, init_code_hash: init_code_hash.into() };

                let (params, addr) = args.find_vanity(generator)?;

                println!("Salt: 0x{}", hex::encode(params.salt));

                addr
            }
        };

        println!("Address: {}", addr);

        println!("Finished in {}s", timer.elapsed().as_secs());

        Ok(addr)
    }
}

/// CLI arguments for `cast wallet vanity`.
#[derive(Debug, Clone, Parser)]
pub struct VanityOpts {
    /// Prefix for the vanity address.
    #[clap(
        long,
        required_unless_present = "ends_with",
        value_parser = HexAddressValidator,
        value_name = "HEX"
    )]
    pub starts_with: Option<String>,

    /// Suffix for the vanity address.
    #[clap(long, value_parser = HexAddressValidator, value_name = "HEX")]
    pub ends_with: Option<String>,

    // 2^64-1 is max possible nonce per [eip-2681](https://eips.ethereum.org/EIPS/eip-2681).
    /// Generate a vanity contract address created by the generated keypair with the specified
    /// nonce.
    #[clap(long)]
    pub nonce: Option<u64>,

    /// Case sensitive matching.
    #[clap(short, long)]
    case_sensitive: bool,
}

impl VanityOpts {
    fn find_vanity<P: Sync + Send, G: WalletGenerator<Params = P>>(
        self,
        generator: G,
    ) -> Result<(P, Address)> {
        let Self { starts_with, ends_with, nonce, case_sensitive } = self;
        let mut left_exact_hex = None;
        let mut left_regex = None;
        let mut right_exact_hex = None;
        let mut right_regex = None;

        if let Some(prefix) = starts_with {
            let decoded = hex::decode(prefix.clone());
            if !case_sensitive && decoded.is_ok() {
                left_exact_hex = Some(decoded.unwrap());
            } else {
                left_regex = Some(Regex::new(&format!(r"^{prefix}"))?);
            }
        }

        if let Some(suffix) = ends_with {
            let decoded = hex::decode(suffix.clone());
            if !case_sensitive && decoded.is_ok() {
                right_exact_hex = Some(decoded.unwrap());
            } else {
                right_regex = Some(Regex::new(&format!(r"{suffix}$"))?);
            }
        }

        macro_rules! find_vanity {
            ($m:ident, $g:ident, $nonce: ident) => {
                if let Some(nonce) = $nonce {
                    find_vanity_address_with_nonce($m, $g, nonce)
                } else {
                    find_vanity_address($m, $g)
                }
            };
        }

        println!("Starting to generate vanity address...");
        let wallet = match (left_exact_hex, left_regex, right_exact_hex, right_regex) {
            (Some(left), _, Some(right), _) => {
                let matcher = HexMatcher { left, right };
                find_vanity!(matcher, generator, nonce)
            }
            (Some(left), _, _, Some(right)) => {
                let matcher = LeftExactRightRegexMatcher { left, right };
                find_vanity!(matcher, generator, nonce)
            }
            (_, Some(left), _, Some(right)) => {
                let matcher = RegexMatcher { left, right };
                find_vanity!(matcher, generator, nonce)
            }
            (_, Some(left), Some(right), _) => {
                let matcher = LeftRegexRightExactMatcher { left, right };
                find_vanity!(matcher, generator, nonce)
            }
            (Some(left), None, None, None) => {
                let matcher = LeftHexMatcher { left };
                find_vanity!(matcher, generator, nonce)
            }
            (None, None, Some(right), None) => {
                let matcher = RightHexMatcher { right };
                find_vanity!(matcher, generator, nonce)
            }
            (None, Some(re), None, None) => {
                let matcher = SingleRegexMatcher { re };
                find_vanity!(matcher, generator, nonce)
            }
            (None, None, None, Some(re)) => {
                let matcher = SingleRegexMatcher { re };
                find_vanity!(matcher, generator, nonce)
            }
            _ => unreachable!(),
        }
        .expect("failed to generate vanity wallet");

        Ok(wallet)
    }
}

/// Generates random wallets until `matcher` matches the wallet address, returning the wallet.
pub fn find_vanity_address<P: Sync + Send, T: VanityMatcher, G: WalletGenerator<Params = P>>(
    matcher: T,
    generator: G,
) -> Option<(P, Address)> {
    wallet_generator(generator).find_any(|(_, addr)| matcher.is_match(addr))
}

/// Generates random wallets until `matcher` matches the contract address created at `nonce`,
/// returning the wallet.
pub fn find_vanity_address_with_nonce<
    P: Sync + Send,
    T: VanityMatcher,
    G: WalletGenerator<Params = P>,
>(
    matcher: T,
    generator: G,
    nonce: u64,
) -> Option<(P, Address)> {
    wallet_generator(generator).find_any(|(_, addr)| {
        let contract_addr = addr.create(nonce);
        matcher.is_match(&contract_addr)
    })
}

/// Returns an infinite parallel iterator which yields a [GeneratedWallet].
#[inline]
pub fn wallet_generator<P: Sync + Send, G: WalletGenerator<Params = P>>(
    generator: G,
) -> impl iter::ParallelIterator<Item = (P, Address)> {
    iter::repeat(()).map(move |_| generator.generate())
}

/// A trait to generate wallets.
pub trait WalletGenerator: Send + Sync + Copy {
    type Params;

    fn generate(&self) -> (Self::Params, Address);
}

#[derive(Debug, Copy, Clone)]
pub struct PrivateKeyGenerator;

#[derive(Debug, Copy, Clone)]
pub struct MnemonicGenerator {
    pub words: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct Create2Generator {
    pub deployer: Address,
    pub init_code_hash: [u8; 32],
}

pub struct PrivateKeyParams {
    pub key: SigningKey,
}

pub struct Create2Params {
    pub salt: [u8; 32],
}

pub struct MnemonicParams {
    pub mnemonic: Mnemonic<English>,
}

impl WalletGenerator for PrivateKeyGenerator {
    type Params = PrivateKeyParams;

    fn generate(&self) -> (Self::Params, Address) {
        let key = SigningKey::random(&mut thread_rng());
        let addr = secret_key_to_address(&key).to_alloy();
        (PrivateKeyParams { key }, addr)
    }
}

impl WalletGenerator for MnemonicGenerator {
    type Params = MnemonicParams;

    fn generate(&self) -> (Self::Params, Address) {
        let mnemonic = Mnemonic::<English>::new_with_count(&mut thread_rng(), self.words).unwrap();
        let derivation_path = "m/44'/60'/0'/0/0"; // first address of default derivation path
        let derived_priv_key = mnemonic.derive_key(derivation_path, None).unwrap();
        let addr = secret_key_to_address(derived_priv_key.as_ref()).to_alloy();
        println!("{}", addr);

        (MnemonicParams { mnemonic }, addr)
    }
}

impl WalletGenerator for Create2Generator {
    type Params = Create2Params;

    fn generate(&self) -> (Self::Params, Address) {
        let salt = thread_rng().gen::<[u8; 32]>();
        let addr = self.deployer.create2(salt, self.init_code_hash);
        (Create2Params { salt }, addr)
    }
}

/// A trait to match vanity addresses.
pub trait VanityMatcher: Send + Sync {
    fn is_match(&self, addr: &Address) -> bool;
}

/// Matches start and end hex.
pub struct HexMatcher {
    pub left: Vec<u8>,
    pub right: Vec<u8>,
}

impl VanityMatcher for HexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let bytes = addr.0.as_slice();
        bytes.starts_with(&self.left) && bytes.ends_with(&self.right)
    }
}

/// Matches only start hex.
pub struct LeftHexMatcher {
    pub left: Vec<u8>,
}

impl VanityMatcher for LeftHexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let bytes = addr.0.as_slice();
        bytes.starts_with(&self.left)
    }
}

/// Matches only end hex.
pub struct RightHexMatcher {
    pub right: Vec<u8>,
}

impl VanityMatcher for RightHexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let bytes = addr.0.as_slice();
        bytes.ends_with(&self.right)
    }
}

/// Matches start hex and end regex.
pub struct LeftExactRightRegexMatcher {
    pub left: Vec<u8>,
    pub right: Regex,
}

impl VanityMatcher for LeftExactRightRegexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let bytes = addr.0.as_slice();
        bytes.starts_with(&self.left) && self.right.is_match(&hex::encode(bytes))
    }
}

/// Matches start regex and end hex.
pub struct LeftRegexRightExactMatcher {
    pub left: Regex,
    pub right: Vec<u8>,
}

impl VanityMatcher for LeftRegexRightExactMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let bytes = addr.0.as_slice();
        bytes.ends_with(&self.right) && self.left.is_match(&hex::encode(bytes))
    }
}

/// Matches a single regex.
pub struct SingleRegexMatcher {
    pub re: Regex,
}

impl VanityMatcher for SingleRegexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let addr = addr.to_checksum(None).strip_prefix("0x").unwrap().to_owned();
        self.re.is_match(&addr)
    }
}

/// Matches start and end regex.
pub struct RegexMatcher {
    pub left: Regex,
    pub right: Regex,
}

impl VanityMatcher for RegexMatcher {
    #[inline]
    fn is_match(&self, addr: &Address) -> bool {
        let addr = addr.to_checksum(None).strip_prefix("0x").unwrap().to_owned();
        self.left.is_match(&addr) && self.right.is_match(&addr)
    }
}

/// Parse 40 byte addresses
#[derive(Copy, Clone, Debug, Default)]
pub struct HexAddressValidator;

impl TypedValueParser for HexAddressValidator {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        if value.len() > 40 {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                "vanity patterns length exceeded. cannot be more than 40 characters",
            ))
        }
        let value = value.to_str().ok_or_else(|| {
            clap::Error::raw(clap::error::ErrorKind::InvalidUtf8, "address must be valid utf8")
        })?;
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_simple_vanity_start() {
        let args = VanitySubcommands::parse_from(["foundry-cli", "--starts-with", "00"]);
        let wallet = args.run().unwrap();
        let addr = wallet;
        let addr = format!("{addr:x}");
        assert!(addr.starts_with("00"));
    }

    #[test]
    fn find_simple_vanity_start2() {
        let args = VanitySubcommands::parse_from(["foundry-cli", "--starts-with", "9"]);
        let wallet = args.run().unwrap();
        let addr = wallet;
        let addr = format!("{addr:x}");
        assert!(addr.starts_with('9'));
    }

    #[test]
    fn find_simple_vanity_end() {
        let args = VanitySubcommands::parse_from(["foundry-cli", "--ends-with", "00"]);
        let wallet = args.run().unwrap();
        let addr = wallet;
        let addr = format!("{addr:x}");
        assert!(addr.ends_with("00"));
    }
}
