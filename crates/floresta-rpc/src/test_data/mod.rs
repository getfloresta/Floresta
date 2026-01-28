//! Instantiating test data.
//!
//! We have some json files that normally helps us to guarantee schema and behavior consistence.

#![cfg_attr(test, allow(dead_code))]

use std::str::FromStr;

use corepc_types::v30::GetBlockVerboseOne;
use serde_json::from_value;
use serde_json::Value;

use crate::rpc_types::GetBlockRes;

/// Internal helper to return expected data to getblock.
///
/// We only support two types of verbosity, use false for zero and true for one.
pub fn getblock_data(verbose: bool) -> GetBlockRes {
    let obj = Value::from_str(include_str!("getblock.json")).unwrap();

    match verbose {
        true => {
            let json = obj.get("getblock_regtestgenesis_verboseone").unwrap();

            let cast: GetBlockVerboseOne = from_value(json.clone()).unwrap();
            GetBlockRes::One(cast.into())
        }
        false => {
            let json = obj.get("getblock_regtestgenesis_verbosezero").unwrap();

            let cast = from_value(json.clone()).unwrap();
            GetBlockRes::Zero(cast)
        }
    }
}
