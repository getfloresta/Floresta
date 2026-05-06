// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;
use std::str::FromStr;

/// Defines an RPC method enum and implements case-insensitive parsing,
/// lowercase string conversion, and `Deref<Target = str>`.
///
/// The generated enum variants are converted to lowercase method names
/// (e.g., `GetBlockHash` -> `"getblockhash"`).
macro_rules! define_rpc_methods {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $enum_name {
            $($variant),*
        }

        impl FromStr for $enum_name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(
                    if s.to_lowercase() == stringify!($variant).to_lowercase() {
                        return Ok(Self::$variant);
                    }
                )*
                Err(format!("Unknown method: {}", s))
            }
        }

        impl $enum_name {
            pub fn to_string(&self) -> String {
                match self {
                    $(Self::$variant => stringify!($variant).to_lowercase(),)*
                }
            }

            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant).to_lowercase().leak(),)*
                }
            }
        }

        impl std::ops::Deref for $enum_name {
            type Target = str;

            fn deref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

define_rpc_methods!(RpcMethods {
    // Blockchain
    FindTxOut,
    GetBestBlockHash,
    GetBlock,
    GetBlockFromPeer,
    GetBlockchainInfo,
    GetBlockCount,
    GetBlockHash,
    GetDeploymentInfo,
    GetDifficulty,
    GetTxOut,
    GetTxOutProof,
    GetRoots,
    GetBlockHeader,

    // Wallet
    LoadDescriptor,
    ListDescriptors,
    RescanBlockchain,

    // Network
    AddNode,
    DisconnectNode,
    GetAddrManInfo,
    GetConnectionCount,
    GetNetworkInfo,
    GetPeerInfo,
    Ping,

    // RawTransactions
    SendRawTransaction,
    GetRawTransaction,

    // Control
    Stop,
    Uptime,
    GetMemoryInfo,
    GetRpcInfo,
});

#[cfg(test)]
mod tests {
    use super::*;

    define_rpc_methods!(TestRpcMethods {
        OneTestMethod,
        TwoTestMethod,
    });

    #[test]
    fn test_macro_enum_creation() {
        let _ = TestRpcMethods::OneTestMethod;
        let _ = TestRpcMethods::TwoTestMethod;
    }

    #[test]
    fn test_macro_from_str() {
        assert_eq!(
            "onetestmethod".parse::<TestRpcMethods>(),
            Ok(TestRpcMethods::OneTestMethod)
        );
        assert_eq!(
            "twotestmethod".parse::<TestRpcMethods>(),
            Ok(TestRpcMethods::TwoTestMethod)
        );
        assert!("invalid".parse::<TestRpcMethods>().is_err());
    }

    #[test]
    fn test_macro_to_string() {
        assert_eq!(TestRpcMethods::OneTestMethod.to_string(), "onetestmethod");
        assert_eq!(TestRpcMethods::TwoTestMethod.to_string(), "twotestmethod");
    }

    #[test]
    fn test_macro_roundtrip() {
        let method_str = TestRpcMethods::OneTestMethod.to_string();
        assert_eq!(
            TestRpcMethods::from_str(&method_str),
            Ok(TestRpcMethods::OneTestMethod)
        );
    }

    #[test]
    fn test_macro_as_str() {
        assert_eq!(TestRpcMethods::OneTestMethod.as_str(), "onetestmethod");
        assert_eq!(TestRpcMethods::TwoTestMethod.as_str(), "twotestmethod");
    }

    #[test]
    fn test_macro_deref() {
        let method = TestRpcMethods::OneTestMethod;
        let dereferenced: &str = &method;
        assert_eq!(dereferenced, "onetestmethod");
    }

    #[test]
    fn test_macro_deref_str_methods() {
        let method = TestRpcMethods::TwoTestMethod;
        assert_eq!(method.len(), "twotestmethod".len());
        assert!(method.starts_with("two"));
        assert!(method.ends_with("method"));
        assert_eq!(method.to_uppercase(), "TWOTESTMETHOD");
    }

    #[test]
    fn test_macro_as_str_matches_to_string() {
        let method = TestRpcMethods::OneTestMethod;
        assert_eq!(method.as_str(), method.to_string().as_str());
    }
}
