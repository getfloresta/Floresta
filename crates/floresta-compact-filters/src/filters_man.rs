// SPDX-License-Identifier: MIT OR Apache-2.0

/// A manager for everything related to Compact Block Filters
///
/// This module wraps every logic related to BIP157/158 Compact Block Filters, such as rescanning,
/// downloading, building and validating filters. It was built with the following principles in
/// mind:
///  - Avoid leaking business logic within `floresta-wire`; the old module was too coupled with
///    wire and that hurts separation of concerns. This is specifically problematic considering
///    that wire and filters are both low-level crates.
///  - Be an independent task that handles everything internally: Don't ask users to do anything
///    post-initialization, all should be handled internally. We download filters, receive new
///    blocks and perform rescan, everything contained and abstracted away from users.
///  - Be careful when downloading blocks: We might rescan a wallet with thousands of hits, that
///    will cause us to potentially download gigabytes of data. This is pretty much a guaranteed
///    self-DoS. For that reason, a pagination algorithm was introduced for retrieving rescan
///    blocks.
///
/// The main struct in this module [`FiltersMan`] will need three things:
///  - A [`NodeHanle`] to request filters, blocks and filter headers,
///  - a [`SafeChain`] to request blockchain related stuff, such as block hashes and block height
///  - and finally, a [`FlilterHeaderStore`] to store filter headers
///
/// After our chain has downloaded all block headers, we start downloading block filter headers.
/// [`FilterHeader`]s are a chained hash that contains the previous filter and the actual filter
/// data. This can be used to verify whether the filter we received is valid. With the exception
/// of filters we decide to cache, all filters will be downloaded from the network when
/// rescanning, and matched against our locally stored header.
///
/// To initialize this module, just create a new [`FilterManHandle`], get a couple of handles to
/// use (you can ask for as many as you need, they are really cheap) and then spawn the main loop
/// as a `tokio` task.
///
/// ### Interface
///
/// Since [`FiltersMan`] is a service, you can't own it. You should use the [`FilterManHandle`],
/// obtained using [`FiltersMan::get_handle`] to communicate with it. Using this handle you can
/// request a rescan, fetch the state of a rescan and get block hits from it to find your
/// transactions.
///
/// When you start a rescan, you can provide one or more scripts to rescan — passing no scripts is
/// considered an error. You may also give a start and end heights for the rescan, if you know
/// where to look for, things might be way faster. This function will then return a
/// [`RescanTicket`] that you should keep. Every time you want to fetch something related to that
/// specific rescan, you must use the ticket. You can follow the state of a rescan by calling
/// `get_state`, if it tells you there are blocks available, you can use `get_blocks` to fetch
/// them. By default, we will download as many blocks as we can, and return all of them here.
/// However, if you expect your wallet to have too many hits, use the `max_blocks_per_page` option
/// to limit how many blocks we keep in memory at the same time. Remember to set a conservative
/// number, since we might actually have this times two, since after you pull a page worth of
/// blocks we will instantly ask for more blocks.
///
/// So this is the flow for a rescan:
/// ```
///                            Start
///                              |
///        Blocks Available  < ---- >   Waiting
///                             |
///                         Finished
/// ```
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

use bitcoin::hashes::Hash;
use bitcoin::p2p::message_filter::CFHeaders;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::FilterHeader;
use bitcoin::ScriptBuf;
use floresta_chain::BlockchainError;
use floresta_chain::BlockchainInterface;
use floresta_chain::ThreadSafeChain;
use floresta_common::impl_error_from;
use floresta_common::try_and_log;
use floresta_wire::node_interface::NodeInterface;
use rand::random;
use tokio::runtime::Handle;
use tokio::select;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::debug;
use tracing::info;

use crate::FilterHeadersStore;
use crate::FlatFilterStoreError;
const MAX_HEADERS_TO_DOWNLOAD: u32 = 1_000;

#[derive(Debug)]
/// All the data provided when requesting a rescan
pub struct RescanRequest {
    /// The actual scripts being rescaned for.
    ///
    /// You may pass as many scripts as you want, the rescan process is highly batchable so you
    /// won't have worse perf with a bigger vec here. Just remember to limit the max number of
    /// blocks we downlaod at a time, to avoid memory exaustion.
    spks: Vec<ScriptBuf>,

    /// The height we should start rescanning.
    ///
    /// If present, we will start the rescan from that height. Start from genesis otherwise.
    start_height: Option<u32>,

    /// The height we should stop rescanning.
    ///
    /// If present, we will stop our rescan process at that height. Go all the way to tip
    /// otherwise.
    end_height: Option<u32>,

    /// How many blocks should we downlaod at a time.
    ///
    /// To avoid taking all our memory with blocks for big wallets, we may download blocks in
    /// chunks, and you can read each chunk using the handle. This option selects how many blocks
    /// each page should have. Uncapped if this is [`None`]
    max_blocks_per_page: Option<u32>,
}

#[derive(Debug)]
/// A rescan that we still haven't finished
///
/// This struct is used to keep track of all rescans that we are still doing, and all relevant info
/// about them.
struct InflightRescan {
    /// Which blocks did the filters match.
    blocks: Vec<BlockHash>,

    /// The original request data that lead to this rescan
    request: RescanRequest,

    /// How many blocks have we delivered already.
    delivered: usize,
}

/// All requests supported by the [`FiltersMan`].
pub enum Requests {
    /// Start a new rescan
    ///
    /// This will tell [`FiltersMan`] to start a new rescan. It will return a [`Responses::RescanStarted`],
    /// with a [`RescanTicket`] used to interact with this rescan.
    ///
    /// Note: This returns after the rescan get schedule, it won't block until it finishes.
    Rescan(RescanRequest),

    /// Fetches the status of a given rescan.
    RescanStatus(RescanTicket),

    /// Request blocks that matches a given rescan criteria.
    Blocks(RescanTicket),

    /// This is used by handle when it gets dropped, so we can remove their channels from our
    /// channels book.
    Shutdown,
}

/// A wrapper over [`Requests`] that also contains a handle_id.
///
/// Each handle holds a pair of [`Sender`] and [`Receiver`] used by [`FiltersMan`] to send
/// responses to each request. The handle will hold a [`Receiver`], and [`FiltersMan`] a
/// [`Sender`]. Since we might have any number of handles, this means we need to figure out
/// which [`Sender`] to use. We do this using the `handle_id`.
pub struct RequestMessage {
    /// An unique identifier for the handle that sent this request.
    handle_id: usize,

    /// The actual [`Requests`] message.
    request: Requests,
}

/// The data sent from [`FiltersMan`] in response to a request.
pub enum Responses {
    /// Sent as a reponse to [`Requests::Rescan`].
    RescanStarted(RescanTicket),

    /// Sent as a reponse to [`Requests::RescanStatus`].
    RescanStatus(RescanStatus),

    /// Sent as a reponse to [`Requests::Blocks`].
    Blocks(Vec<Block>),

    /// Sent as a reponse to [`Requests::RescanStatus`] and [`Requests::Blocks`] if we can't find
    /// that rescan.
    NotFound(RescanTicket),
}

pub enum RescanStatus {
    /// Rescan started, but there are no available blocks yet
    Started,

    /// We have some available blocks
    Available,

    /// We are still downloading blocks
    Waiting,

    /// No more blocks to fetch
    Finished,
}

impl Display for RescanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RescanStatus::Started => {
                write!(f, "Rescan started, waiting for the first available block")
            }
            RescanStatus::Available => write!(
                f,
                "Blocks are available for reading, use `Requests::Blocks` to fetch them"
            ),
            RescanStatus::Waiting => {
                write!(f, "Still waiting for some extra blocks to be downloaded")
            }
            RescanStatus::Finished => write!(
                f,
                "This rescan has finished, and there are no more blocks of interest"
            ),
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
pub struct RescanTicket(u32);

/// A manager that downloads filter headers and performs rescan on behalf of
/// users
pub struct FiltersMan<Store: FilterHeadersStore, Chain: ThreadSafeChain> {
    /// A store where we can store our filter headers
    store: Arc<Mutex<Store>>,

    /// A node interface used to request filters, filter headers and blocks
    node: NodeInterface,

    /// A [BlockchainInterface] used to know the best chain state
    chain: Chain,

    request_rx: Receiver<RequestMessage>,
    request_tx: Sender<RequestMessage>,

    handles: HashMap<usize, Sender<Responses>>,
    handle_id_count: usize,

    inflight_rescans: HashMap<RescanTicket, InflightRescan>,
    blocks: HashMap<BlockHash, Block>,
    pending_rescans: Vec<Receiver<InflightRescan>>,
}

#[derive(Debug)]
pub enum FilterManError {
    Chain(BlockchainError),
    Wire(Box<dyn Error + Send + Sync>),
    Store(FlatFilterStoreError),
    PoisonedLock,
}

impl<Store> From<PoisonError<MutexGuard<'_, Store>>> for FilterManError {
    fn from(_: PoisonError<MutexGuard<'_, Store>>) -> Self {
        Self::PoisonedLock
    }
}

impl_error_from!(FilterManError, BlockchainError, Chain);
impl_error_from!(FilterManError, FlatFilterStoreError, Store);

#[derive(Debug)]
/// A helper struct that informs what is the next filter to download during filters sync
struct NextToDownload {
    height: u32,
    stop_hash: BlockHash,
}

impl Display for NextToDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "height: {}, stop_hash: {}", self.height, self.stop_hash)
    }
}

impl<Store: FilterHeadersStore, Chain: ThreadSafeChain + Clone> FiltersMan<Store, Chain>
where
    FilterManError: From<<Chain as BlockchainInterface>::Error>,
{
    pub fn new(store: Store, node: NodeInterface, chain: Chain) -> Self {
        let (request_tx, request_rx) = channel(1024);

        Self {
            store: Arc::new(Mutex::new(store)),
            node,
            chain,
            request_rx,
            request_tx,
            inflight_rescans: HashMap::new(),
            blocks: HashMap::new(),
            pending_rescans: Vec::new(),
            handles: HashMap::new(),
            handle_id_count: 0,
        }
    }

    pub fn get_handle(&mut self) -> FilterManHandle {
        let (sender, receiver) = channel(1024);
        self.handle_id_count += 1;
        self.handles.insert(self.handle_id_count, sender);

        FilterManHandle {
            id: self.handle_id_count,
            sender: self.request_tx.clone(),
            receiver,
        }
    }

    async fn handle_request(&mut self, request: RequestMessage) -> Result<(), FilterManError> {
        let Some(response_channel) = self.handles.get(&request.handle_id) else {
            todo!()
        };

        match request.request {
            Requests::Rescan(rescan_request) => {
                let id = random();
                let ticket = RescanTicket(id);

                let (sender, receiver) = channel(1024);
                self.pending_rescans.push(receiver);

                // Start a task that will do the rescan
                let store = self.store.clone();
                let chain = self.chain.clone();
                let node = self.node.clone();

                tokio::task::spawn_blocking(move || {
                    try_and_log!(Self::do_filters_rescan(
                        store,
                        rescan_request,
                        sender,
                        chain,
                        node,
                    ));
                });

                try_and_log!(
                    response_channel
                        .send(Responses::RescanStarted(ticket))
                        .await
                );
            }

            Requests::Blocks(rescan_ticket) => {
                let blocks = &self.blocks;
                let Some(inflight_rescan) = self.inflight_rescans.get_mut(&rescan_ticket) else {
                    try_and_log!(
                        response_channel
                            .send(Responses::NotFound(rescan_ticket))
                            .await
                    );
                    return Ok(());
                };

                let ready_blocks = inflight_rescan.blocks[inflight_rescan.delivered..]
                    .iter()
                    .filter_map(|block| blocks.get(block))
                    .take(50) // TODO
                    .cloned()
                    .collect::<Vec<_>>();

                let n_ready_blocks = ready_blocks.len();
                try_and_log!(response_channel.send(Responses::Blocks(ready_blocks)).await);
                inflight_rescan.delivered += n_ready_blocks;
            }

            Requests::RescanStatus(rescan_ticket) => {
                let Some(inflight_rescan) = self.inflight_rescans.get(&rescan_ticket) else {
                    try_and_log!(
                        response_channel
                            .send(Responses::NotFound(rescan_ticket))
                            .await
                    );

                    return Ok(());
                };

                // We are probably still rescanning with filters, nothing to send
                if inflight_rescan.delivered == 0 {
                    try_and_log!(
                        response_channel
                            .send(Responses::RescanStatus(RescanStatus::Started))
                            .await
                    );
                }

                // We've sent everything already, nothing else to do
                if inflight_rescan.delivered == inflight_rescan.blocks.len() {
                    try_and_log!(
                        response_channel
                            .send(Responses::RescanStatus(RescanStatus::Finished))
                            .await
                    );
                }

                let ready_blocks = inflight_rescan.blocks[inflight_rescan.delivered..]
                    .iter()
                    .filter(|block| self.blocks.contains_key(*block))
                    .count();

                // We have available blocks
                if ready_blocks > 0 {
                    try_and_log!(
                        response_channel
                            .send(Responses::RescanStatus(RescanStatus::Available))
                            .await
                    );
                    return Ok(());
                }

                // No more available blocks
                try_and_log!(
                    response_channel
                        .send(Responses::RescanStatus(RescanStatus::Waiting))
                        .await
                );
            }

            Requests::Shutdown => {
                self.handles.remove(&request.handle_id);
            }
        }

        Ok(())
    }

    pub async fn main_loop(mut self) -> Result<(), FilterManError>{
        try_and_log!(self.sync().await);

        let current_height = self.store.lock()?.get_height()?.unwrap_or(0);
        info!("Filters manager set up at height {current_height}");

        loop {
            select! {
                request = self.request_rx.recv() => {
                    if let Some(request) = request {
                        try_and_log!(self.handle_request(request).await);
                    }
                }
            }
        }
    }

    fn do_filters_rescan(
        store: Arc<Mutex<Store>>,
        rescan: RescanRequest,
        done_channel: Sender<InflightRescan>,
        chain: Chain,
        node: NodeInterface,
    ) -> Result<(), FilterManError> {
        let height = chain.get_height().unwrap_or(0);
        let start = rescan.start_height.unwrap_or(0);

        let stop = rescan.end_height.unwrap_or(height);

        let mut query = Vec::new();
        query.extend(rescan.spks.iter().map(|s| s.as_bytes()));

        let mut blocks = Vec::new();
        for height in start..stop {
            let mut store = store.lock()?;
            // This inner loop prevents the main thread from starving due to this task holding the
            // lock for too much time. It's basically a poor-man's yield to allow others to access
            // `store`. Without it, other tasks (including the main one and other rescans) would
            // starve and not be able to make progress until we finish working.
            for _ in 0..500 {
                let block_hash = chain.get_block_hash(height)?;
                let header = store.get_filter_header(height);
                let filter = store.get_filter(height)?.unwrap();

                // TODO(Davidson): how tf do we get rid of this `clone`???????
                if filter
                    .match_any(&block_hash, query.clone().into_iter())
                    .unwrap()
                {
                    blocks.push(block_hash);
                }
            }
        }

        let done_cb = async move {
            try_and_log!(
                done_channel
                    .send(InflightRescan {
                        blocks,
                        request: rescan,
                        delivered: 0,
                    })
                    .await
            );
        };

        tokio::spawn(done_cb);
        Ok(())
    }

    fn apply_headers(&mut self, cf_headers: CFHeaders) -> Result<(), FilterManError> {
        let mut store = self.store.lock()?;
        let last_stored_header = store
            .get_height()?
            .map(|h| Ok::<FilterHeader, FilterManError>(store.get_filter_header(h)?))
            .transpose()?
            .unwrap_or(FilterHeader::all_zeros());

        if cf_headers.previous_filter_header != last_stored_header {
            // todo: handle this
            panic!("Received filter headers that don't connect to our current chain of filters");
        }

        let mut current_header = last_stored_header;
        for filter_hash in cf_headers.filter_hashes {
            current_header = filter_hash.filter_header(&current_header);
            store.put_filter_header(current_header)?;
        }

        store.flush()?;
        Ok(())
    }

    /// Syncs a filters manger with filter headers
    pub async fn sync(&mut self) -> Result<(), FilterManError> {
        while let Ok(Some(height)) = self.next_to_request() {
            debug!("Requesting filter headers for {}", height);
            let NextToDownload { height, stop_hash } = height;
            let cf_headers = self
                .node
                .get_cfilters_headers(height, stop_hash)
                .await
                .map_err(|e| FilterManError::Wire(e.into()))?;

            self.apply_headers(cf_headers)?;
        }

        Ok(())
    }

    fn next_to_request(&self) -> Result<Option<NextToDownload>, FilterManError> {
        let last_stored = self.store.lock()?.get_height()?.unwrap_or(0);
        let tip_height = self.chain.get_height()?;

        if last_stored >= tip_height {
            return Ok(None);
        }

        let stop_height = last_stored
            .saturating_add(MAX_HEADERS_TO_DOWNLOAD)
            .min(tip_height);

        if last_stored == 0 {
            return Ok(Some(NextToDownload {
                height: last_stored,
                stop_hash: self.chain.get_block_hash(stop_height)?,
            }));
        }

        Ok(Some(NextToDownload {
            height: last_stored + 1,
            stop_hash: self.chain.get_block_hash(stop_height)?,
        }))
    }
}

pub enum FilterManHandlerError {
    Sending,
    Receiving,
    NothingToRescan,
}

pub struct FilterManHandle {
    id: usize,
    sender: Sender<RequestMessage>,
    receiver: Receiver<Responses>,
}

impl FilterManHandle {
    pub async fn rescan(
        &mut self,
        rescan_request: RescanRequest,
    ) -> Result<RescanTicket, FilterManHandlerError> {
        if rescan_request.spks.is_empty() {
            return Err(FilterManHandlerError::NothingToRescan);
        }

        let request = RequestMessage {
            request: Requests::Rescan(rescan_request),
            handle_id: self.id,
        };

        self.sender.send(request).await;

        if let Responses::RescanStarted(ticket) = self
            .receiver
            .recv()
            .await
            .ok_or(FilterManHandlerError::Receiving)?
        {
            return Ok(ticket);
        }

        unreachable!()
    }

    pub async fn get_info(
        &mut self,
        rescan_ticket: RescanTicket,
    ) -> Result<RescanStatus, FilterManHandlerError> {
        let request = RequestMessage {
            request: Requests::RescanStatus(rescan_ticket),
            handle_id: self.id,
        };
        self.sender.send(request).await;

        if let Responses::RescanStatus(status) = self
            .receiver
            .recv()
            .await
            .ok_or(FilterManHandlerError::Receiving)?
        {
            return Ok(status);
        }

        unreachable!()
    }

    pub async fn get_blocks(
        &mut self,
        rescan_ticket: RescanTicket,
    ) -> Result<Vec<Block>, FilterManHandlerError> {
        let request = RequestMessage {
            request: Requests::Blocks(rescan_ticket),
            handle_id: self.id,
        };

        self.sender.send(request).await;

        if let Responses::Blocks(blocks) = self
            .receiver
            .recv()
            .await
            .ok_or(FilterManHandlerError::Receiving)?
        {
            return Ok(blocks);
        }

        unreachable!()
    }
}

// A drop implementation that tells the manager this handle is no longer inside scope
impl Drop for FilterManHandle {
    fn drop(&mut self) {
        // Don't try to tokio::spawn if there's no runtime running.
        // This is generally OK because no runtime = no manager running either.
        if Handle::try_current().is_err() {
            return;
        }

        let request = RequestMessage {
            request: Requests::Shutdown,
            handle_id: self.id,
        };

        // If we can't send a message through, it means the other side of this channel is already
        // dropped, so it doesn't really matter we can't send this
        let channel = std::mem::replace(&mut self.sender, channel(1024).0);
        tokio::spawn(async move {
            let _ = channel.send(request).await;
        });
    }
}
