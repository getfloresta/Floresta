#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef struct {
  bool ok;
  char *value;
} FfiResult;

void floresta_bitassets_string_free(char *value);

FfiResult floresta_bitassets_wallet_open(const char *config_json);
void floresta_bitassets_wallet_free(uintptr_t handle);

FfiResult floresta_bitassets_wallet_get_new_address(uintptr_t handle);
FfiResult floresta_bitassets_wallet_info(uintptr_t handle);
FfiResult floresta_bitassets_wallet_sync(uintptr_t handle);
FfiResult floresta_bitassets_wallet_list_utxos(uintptr_t handle);
FfiResult floresta_bitassets_wallet_get_balance(uintptr_t handle, const char *asset_id);

FfiResult floresta_bitassets_wallet_transfer(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_reserve(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_register(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_amm_mint(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_amm_swap(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_amm_burn(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_dutch_auction_create(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_dutch_auction_bid(uintptr_t handle, const char *params_json);
FfiResult floresta_bitassets_wallet_dutch_auction_collect(uintptr_t handle, const char *params_json);
