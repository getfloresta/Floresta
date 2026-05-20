use std::ffi::{c_char, CStr, CString};
use std::ptr;

use crate::mobile::{EmbeddedBitAssetsWallet, EmbeddedWalletConfig};

#[repr(C)]
pub struct FfiResult {
    pub ok: bool,
    pub value: *mut c_char,
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(value);
    }
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_open(config_json: *const c_char) -> FfiResult {
    ffi_result(|| {
        let config: EmbeddedWalletConfig = serde_json::from_str(read_str(config_json)?)?;
        let wallet = EmbeddedBitAssetsWallet::open(config)?;
        let handle = Box::into_raw(Box::new(wallet)) as usize;
        Ok(handle.to_string())
    })
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_free(handle: usize) {
    if handle == 0 {
        return;
    }
    unsafe {
        let _ = Box::from_raw(handle as *mut EmbeddedBitAssetsWallet);
    }
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_get_new_address(handle: usize) -> FfiResult {
    with_wallet(handle, |wallet| wallet.get_new_address())
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_info(handle: usize) -> FfiResult {
    with_wallet(handle, |wallet| wallet.wallet_info_json())
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_sync(handle: usize) -> FfiResult {
    with_wallet(handle, |wallet| wallet.sync_json())
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_list_utxos(handle: usize) -> FfiResult {
    with_wallet(handle, |wallet| wallet.list_utxos_json())
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_get_balance(
    handle: usize,
    asset_id: *const c_char,
) -> FfiResult {
    ffi_result(|| {
        let wallet = wallet_from_handle(handle)?;
        let asset_id = if asset_id.is_null() {
            None
        } else {
            Some(read_str(asset_id)?)
        };
        Ok(wallet.get_balance_json(asset_id)?)
    })
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_transfer(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::transfer_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_reserve(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::reserve_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_register(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::register_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_amm_mint(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::amm_mint_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_amm_swap(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::amm_swap_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_amm_burn(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(handle, params_json, EmbeddedBitAssetsWallet::amm_burn_json)
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_dutch_auction_create(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(
        handle,
        params_json,
        EmbeddedBitAssetsWallet::dutch_auction_create_json,
    )
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_dutch_auction_bid(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(
        handle,
        params_json,
        EmbeddedBitAssetsWallet::dutch_auction_bid_json,
    )
}

#[no_mangle]
pub extern "C" fn floresta_bitassets_wallet_dutch_auction_collect(
    handle: usize,
    params_json: *const c_char,
) -> FfiResult {
    with_wallet_json(
        handle,
        params_json,
        EmbeddedBitAssetsWallet::dutch_auction_collect_json,
    )
}

fn with_wallet(
    handle: usize,
    f: impl FnOnce(&mut EmbeddedBitAssetsWallet) -> Result<String, crate::Error>,
) -> FfiResult {
    ffi_result(|| {
        let wallet = wallet_from_handle(handle)?;
        Ok(f(wallet)?)
    })
}

fn with_wallet_json(
    handle: usize,
    params_json: *const c_char,
    f: impl FnOnce(&mut EmbeddedBitAssetsWallet, &str) -> Result<String, crate::Error>,
) -> FfiResult {
    ffi_result(|| {
        let params_json = read_str(params_json)?;
        let wallet = wallet_from_handle(handle)?;
        Ok(f(wallet, params_json)?)
    })
}

fn ffi_result(f: impl FnOnce() -> Result<String, Box<dyn std::error::Error>>) -> FfiResult {
    match f() {
        Ok(value) => FfiResult {
            ok: true,
            value: string_to_ptr(value),
        },
        Err(error) => FfiResult {
            ok: false,
            value: string_to_ptr(error.to_string()),
        },
    }
}

fn wallet_from_handle(
    handle: usize,
) -> Result<&'static mut EmbeddedBitAssetsWallet, Box<dyn std::error::Error>> {
    if handle == 0 {
        return Err("BitAssets wallet handle is null".into());
    }
    let wallet = unsafe { (handle as *mut EmbeddedBitAssetsWallet).as_mut() }
        .ok_or("BitAssets wallet handle is invalid")?;
    Ok(wallet)
}

fn read_str(value: *const c_char) -> Result<&'static str, Box<dyn std::error::Error>> {
    if value.is_null() {
        return Err("expected non-null string pointer".into());
    }
    Ok(unsafe { CStr::from_ptr(value) }.to_str()?)
}

fn string_to_ptr(value: String) -> *mut c_char {
    match CString::new(value) {
        Ok(value) => value.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

#[cfg(target_os = "android")]
mod android_jni {
    use std::ffi::{CStr, CString};

    use jni::{
        objects::{JObject, JString},
        sys::{jlong, jstring},
        JNIEnv,
    };
    use serde_json::json;

    use super::{
        floresta_bitassets_wallet_amm_burn, floresta_bitassets_wallet_amm_mint,
        floresta_bitassets_wallet_amm_swap, floresta_bitassets_wallet_dutch_auction_bid,
        floresta_bitassets_wallet_dutch_auction_collect,
        floresta_bitassets_wallet_dutch_auction_create, floresta_bitassets_wallet_free,
        floresta_bitassets_wallet_get_balance, floresta_bitassets_wallet_get_new_address,
        floresta_bitassets_wallet_info, floresta_bitassets_wallet_list_utxos,
        floresta_bitassets_wallet_open, floresta_bitassets_wallet_register,
        floresta_bitassets_wallet_reserve, floresta_bitassets_wallet_sync,
        floresta_bitassets_wallet_transfer, FfiResult,
    };

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeOpen(
        mut env: JNIEnv,
        _this: JObject,
        config_json: JString,
    ) -> jstring {
        let config_json = java_string_to_rust(&mut env, config_json);
        result_to_java_string(
            &mut env,
            match config_json {
                Ok(config_json) => with_c_string(&config_json, floresta_bitassets_wallet_open),
                Err(error) => json_error(error),
            },
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeFree(
        _env: JNIEnv,
        _this: JObject,
        handle: jlong,
    ) {
        floresta_bitassets_wallet_free(handle as usize);
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeGetNewAddress(
        mut env: JNIEnv,
        _this: JObject,
        handle: jlong,
    ) -> jstring {
        result_to_java_string(
            &mut env,
            result_json(floresta_bitassets_wallet_get_new_address(handle as usize)),
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeWalletInfo(
        mut env: JNIEnv,
        _this: JObject,
        handle: jlong,
    ) -> jstring {
        result_to_java_string(
            &mut env,
            result_json(floresta_bitassets_wallet_info(handle as usize)),
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeSync(
        mut env: JNIEnv,
        _this: JObject,
        handle: jlong,
    ) -> jstring {
        result_to_java_string(
            &mut env,
            result_json(floresta_bitassets_wallet_sync(handle as usize)),
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeListUtxos(
        mut env: JNIEnv,
        _this: JObject,
        handle: jlong,
    ) -> jstring {
        result_to_java_string(
            &mut env,
            result_json(floresta_bitassets_wallet_list_utxos(handle as usize)),
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeGetBalance(
        mut env: JNIEnv,
        _this: JObject,
        handle: jlong,
        asset_id: JString,
    ) -> jstring {
        let asset_id = java_string_to_rust(&mut env, asset_id);
        result_to_java_string(
            &mut env,
            match asset_id {
                Ok(asset_id) if asset_id.is_empty() => {
                    result_json(floresta_bitassets_wallet_get_balance(handle as usize, std::ptr::null()))
                }
                Ok(asset_id) => with_c_string(&asset_id, |asset_id| {
                    floresta_bitassets_wallet_get_balance(handle as usize, asset_id)
                }),
                Err(error) => json_error(error),
            },
        )
    }

    macro_rules! json_method {
        ($name:ident, $rust_fn:ident) => {
            #[no_mangle]
            pub extern "system" fn $name(
                mut env: JNIEnv,
                _this: JObject,
                handle: jlong,
                params_json: JString,
            ) -> jstring {
                let params_json = java_string_to_rust(&mut env, params_json);
                result_to_java_string(
                    &mut env,
                    match params_json {
                        Ok(params_json) => with_c_string(&params_json, |params_json| {
                            $rust_fn(handle as usize, params_json)
                        }),
                        Err(error) => json_error(error),
                    },
                )
            }
        };
    }

    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeTransfer,
        floresta_bitassets_wallet_transfer
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeReserve,
        floresta_bitassets_wallet_reserve
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeRegister,
        floresta_bitassets_wallet_register
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeAmmMint,
        floresta_bitassets_wallet_amm_mint
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeAmmSwap,
        floresta_bitassets_wallet_amm_swap
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeAmmBurn,
        floresta_bitassets_wallet_amm_burn
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeDutchAuctionCreate,
        floresta_bitassets_wallet_dutch_auction_create
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeDutchAuctionBid,
        floresta_bitassets_wallet_dutch_auction_bid
    );
    json_method!(
        Java_io_bluewallet_bluewallet_BitAssetsWalletModule_nativeDutchAuctionCollect,
        floresta_bitassets_wallet_dutch_auction_collect
    );

    fn java_string_to_rust(env: &mut JNIEnv, value: JString) -> Result<String, String> {
        env.get_string(&value)
            .map(|value| value.into())
            .map_err(|error| error.to_string())
    }

    fn with_c_string(value: &str, f: impl FnOnce(*const i8) -> FfiResult) -> String {
        match CString::new(value) {
            Ok(value) => result_json(f(value.as_ptr())),
            Err(error) => json_error(error.to_string()),
        }
    }

    fn result_json(result: FfiResult) -> String {
        let value = if result.value.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(result.value) }
                .to_string_lossy()
                .into_owned()
        };
        if !result.value.is_null() {
            unsafe {
                let _ = CString::from_raw(result.value);
            }
        }
        json!({ "ok": result.ok, "value": value }).to_string()
    }

    fn json_error(error: String) -> String {
        json!({ "ok": false, "value": error }).to_string()
    }

    fn result_to_java_string(env: &mut JNIEnv, value: String) -> jstring {
        env.new_string(value)
            .map(|value| value.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }
}
