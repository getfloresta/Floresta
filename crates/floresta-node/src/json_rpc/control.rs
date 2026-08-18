// SPDX-License-Identifier: MIT OR Apache-2.0

use floresta_rpc::rpc_interfaces::ControlRpc;
use floresta_rpc::rpc_types::ActiveCommand;
use floresta_rpc::rpc_types::GetMemInfoRes;
use floresta_rpc::rpc_types::GetMemInfoStats;
use floresta_rpc::rpc_types::GetRpcInfoRes;
use floresta_rpc::rpc_types::MemInfoLocked;

use super::res::jsonrpc_interface::JsonRpcError;
use super::server::RpcChain;
use super::server::RpcImpl;

impl<Blockchain: RpcChain> ControlRpc for RpcImpl<Blockchain> {
    type Error = JsonRpcError;

    async fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes, JsonRpcError> {
        #[cfg(target_env = "gnu")]
        match mode.as_str() {
            "stats" => {
                let info = unsafe { libc::mallinfo() };

                let stats = GetMemInfoStats {
                    locked: MemInfoLocked {
                        used: info.uordblks as u64,
                        free: info.fordblks as u64,
                        total: (info.uordblks + info.fordblks) as u64,
                        locked: info.hblkhd as u64,
                        chunks_used: info.ordblks as u64,
                        chunks_free: info.smblks as u64,
                    },
                };

                Ok(GetMemInfoRes::Stats(stats))
            }

            "mallocinfo" => {
                // A XML with the allocator statistics
                let info = unsafe { libc::mallinfo() };
                let info_str = format!(
                    "<malloc version=\"2.0\"><heap nr=\"1\"><allocated>{}</allocated><free>{}</free><total>{}</total><locked>{}</locked><chunks nr=\"{}\"><used>{}</used><free>{}</free></chunks></heap></malloc>",
                    info.hblkhd,
                    info.uordblks,
                    info.fordblks,
                    info.uordblks + info.fordblks,
                    info.hblkhd,
                    info.ordblks,
                    info.smblks,
                );

                Ok(GetMemInfoRes::MallocInfo(info_str))
            }

            _ => Err(JsonRpcError::InvalidMemInfoMode),
        }

        #[cfg(target_os = "macos")]
        match mode.as_str() {
            "stats" => {
                let mut info: libc::malloc_statistics_t = unsafe { std::mem::zeroed() };
                unsafe {
                    libc::malloc_zone_statistics(std::ptr::null_mut(), &mut info);
                }

                let stats = GetMemInfoStats {
                    locked: MemInfoLocked {
                        used: info.size_in_use as u64,
                        free: info.size_allocated.saturating_sub(info.size_in_use) as u64,
                        total: info.size_allocated as u64,
                        locked: info.size_allocated as u64,
                        chunks_used: info.blocks_in_use as u64,
                        chunks_free: 0, // Not available on MacOS
                    },
                };

                Ok(GetMemInfoRes::Stats(stats))
            }
            "mallocinfo" => {
                // A XML with the allocator statistics
                let mut info: libc::malloc_statistics_t = unsafe { std::mem::zeroed() };
                unsafe {
                    libc::malloc_zone_statistics(std::ptr::null_mut(), &mut info);
                }

                let info_str = format!(
                    "<malloc version=\"2.0\"><heap nr=\"1\"><allocated>{}</allocated><free>{}</free><total>{}</total><locked>{}</locked><chunks nr=\"{}\"><used>{}</used><free>{}</free></chunks></heap></malloc>",
                    info.size_allocated,
                    info.size_in_use,
                    info.size_allocated - info.size_in_use,
                    info.size_allocated,
                    info.size_allocated,
                    info.blocks_in_use,
                    0
                );

                Ok(GetMemInfoRes::MallocInfo(info_str))
            }
            _ => Err(JsonRpcError::InvalidMemInfoMode),
        }

        #[cfg(not(any(target_env = "gnu", target_os = "macos")))]
        // Just return zeroed stats for non-GNU and non-MacOS targets
        match mode.as_str() {
            "stats" => Ok(GetMemInfoRes::Stats(GetMemInfoStats::default())),
            "mallocinfo" => Ok(GetMemInfoRes::MallocInfo(String::new())),
            _ => Err(JsonRpcError::InvalidMemInfoMode),
        }
    }

    async fn get_rpc_info(&self) -> Result<GetRpcInfoRes, JsonRpcError> {
        let active_commands = self
            .inflight
            .read()
            .await
            .values()
            .map(|req| ActiveCommand {
                method: req.method.clone(),
                duration: req.when.elapsed().as_micros() as u64,
            })
            .collect();

        Ok(GetRpcInfoRes {
            active_commands,
            logpath: self.log_path.clone(),
        })
    }

    // help
    // logging

    // stop
    async fn stop(&self) -> Result<String, JsonRpcError> {
        *self.kill_signal.write().await = true;

        Ok("Floresta stopping".to_string())
    }

    // uptime
    async fn uptime(&self) -> Result<u64, JsonRpcError> {
        Ok(self.start_time.elapsed().as_secs())
    }
}
