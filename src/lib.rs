use pyo3::prelude::*;
use tokio::runtime::Runtime;
use chrono::Utc;
use ethers::{abi::Abi, contract::Contract, providers:: { Http, Middleware, Provider}, types::Address};
use serde::{Deserialize, Serialize};
use sha2::{ Digest, Sha256};
use std::{collections::HashSet, sync::Arc};
use serde_json::{self, Number, Value};
use std::marker::Send;
use ethers::types::{Filter, Log, H160, H256, U64, I256, U256, Block, BlockNumber};
use ethers::abi::RawLog;
use ethers::contract::EthLogDecode;
use ethers::contract::EthEvent;
use ethers::utils::hex;

use std::cmp::min;
use std::collections::HashMap;
use std::str::FromStr;
use pyo3::{IntoPy, PyObject};
use pyo3::types::{PyList, PyDict};
use futures::{future::join_all, lock::Mutex};


use num_bigint::BigInt;

const BATCH_SIZE: usize = 10000; // Number of blocks to fetch in each batch
const NUM_BLOCKS: u64 = 100; // Number of blocks to consider for average block time calculation
const FACTORY_ADDRESS: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
const POOL_CREATED_SIGNATURE: &str = "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118";
const SWAP_EVENT_SIGNATURE: &str = "c42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67";
const MINT_EVENT_SIGNATURE: &str = "7a53080ba414158be7ec69b987b5fb7d07dee101fe85488f0853ae16239d0bde";
const BURN_EVENT_SIGNATURE: &str = "0c396cd989a39f4459b5fa1aed6a9a8dcdbc45908acfd67e028cd568da98982c";
const COLLECT_EVENT_SIGNATURE: &str = "70935338e69775456a85ddef226c395fb668b63fa0115f5f20610b388e6ca9c0";
struct PyValue(Value);

impl IntoPy<PyObject> for PyValue {
    fn into_py(self, py: Python) -> PyObject {
        match self.0 {
            Value::Null => py.None(),
            Value::Bool(b) => b.into_py(py),
            Value::Number(n) => n.as_i64().unwrap().into_py(py),
            Value::String(s) => s.into_py(py),
            Value::Array(a) => {
                let py_list = PyList::empty(py);
                for item in a {
                    py_list.append(PyValue(item).into_py(py)).unwrap();
                }
                py_list.into_py(py)
            },
            Value::Object(o) => {
                let py_dict = PyDict::new(py);
                for (k, v) in o {
                    py_dict.set_item(k, PyValue(v).into_py(py)).unwrap();
                }
                py_dict.into_py(py)
            },
        }
    }
}

#[derive(Debug, EthEvent, Serialize, Deserialize)]
#[ethevent(name = "Swap", abi = "Swap(address indexed sender, address indexed to, int256 amount0, int256 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick)")]
struct SwapEvent {
    sender: Address,
    to: Address,
    amount0: I256,
    amount1: I256,
    sqrt_price_x96: U256,
    liquidity: U256,
    tick: i32,  // ABI's int24 can fit in i32 in Rust
}

#[derive(Debug, EthEvent, Serialize, Deserialize)]
#[ethevent(name = "Mint", abi = "Mint(address sender, address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1)")]
struct MintEvent {
    sender: Address,
    owner: Address,
    tick_lower: i32,  // int24 fits in i32
    tick_upper: i32,  // int24 fits in i32
    amount: U256,
    amount0: U256,
    amount1: U256,
}

#[derive(Debug, EthEvent, Serialize, Deserialize)]
#[ethevent(name = "Burn", abi = "Burn(address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1)")]
struct BurnEvent {
    owner: Address,
    tick_lower: i32,  // int24 fits in i32
    tick_upper: i32,  // int24 fits in i32
    amount: U256,
    amount0: U256,
    amount1: U256,
}

#[derive(Debug, EthEvent, Serialize)]
#[ethevent(name = "Collect", abi = "Collect(address indexed owner, address recipient, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount0, uint128 amount1)")]
struct CollectEvent {
    owner: Address,
    recipient: Address,
    tick_lower: i32,  // int24 fits in i32
    tick_upper: i32,  // int24 fits in i32
    amount0: U256,
    amount1: U256,
}

#[derive(Debug, Serialize)]
enum UniswapEvent {
    Swap(SwapEvent),
    Mint(MintEvent),
    Burn(BurnEvent),
    Collect(CollectEvent),
}

impl EthLogDecode for UniswapEvent {
    fn decode_log(log: &RawLog) -> Result<Self, ethers::abi::Error> {
        if let Ok((event, _, _)) = decode_uniswap_event(&Log {
            address: H160::zero(),
            topics: log.topics.clone(),
            data: log.data.clone().into(),
            block_hash: None,
            block_number: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: None,
        }) {
            Ok(event)
        } else {
            Err(ethers::abi::Error::InvalidData)
        }
    }
}

fn decode_uniswap_event(log: &Log) -> Result<(UniswapEvent, H256, u64), Box<dyn std::error::Error + Send + Sync>> {
    // Event signatures for Uniswap V3 pool events
    let swap_signature = H256::from_slice(&hex::decode(SWAP_EVENT_SIGNATURE).unwrap());
    let mint_signature = H256::from_slice(&hex::decode(MINT_EVENT_SIGNATURE).unwrap());
    let burn_signature = H256::from_slice(&hex::decode(BURN_EVENT_SIGNATURE).unwrap());
    let collect_signature = H256::from_slice(&hex::decode(COLLECT_EVENT_SIGNATURE).unwrap());

    // Parse the raw log data
    let raw_log = RawLog {
        topics: log.topics.clone(),
        data: log.data.to_vec(),
    };

    let hash = log.transaction_hash.ok_or("Missing transaction hash")?;
    let block_number = log.block_number.ok_or("Missing block number")?.as_u64();

    // Match based on event signature and decode the appropriate event
    if log.topics[0] == swap_signature {
        match <SwapEvent as EthLogDecode>::decode_log(&raw_log) {
            Ok(event) => return Ok((UniswapEvent::Swap(event), hash, block_number)),
            Err(err) => return Err(Box::new(err)),
        }
    } else if log.topics[0] == mint_signature {
        match <MintEvent as EthLogDecode>::decode_log(&raw_log) {
            Ok(event) => return Ok((UniswapEvent::Mint(event), hash, block_number)),
            Err(err) => return Err(Box::new(err)),
        }
    } else if log.topics[0] == burn_signature {
        match <BurnEvent as EthLogDecode>::decode_log(&raw_log) {
            Ok(event) => return Ok((UniswapEvent::Burn(event), hash, block_number)),
            Err(err) => return Err(Box::new(err)),
        }
    } else if log.topics[0] == collect_signature {
        match <CollectEvent as EthLogDecode>::decode_log(&raw_log) {
            Ok(event) => return Ok((UniswapEvent::Collect(event), hash, block_number)),
            Err(err) => return Err(Box::new(err)),
        }
    } else {
        println!("Unknown event signature: {:?}", log);
    }
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Unknown event signature")))
}


#[derive(Debug, EthEvent, Serialize)]
#[ethevent(name = "PoolCreated", abi = "PoolCreated(address indexed token0, address indexed token1, uint24 indexed fee, int24 tickSpacing, address pool)")]
struct PoolCreatedEvent {
    token0: Address,
    token1: Address,
    fee: u32,
    tick_spacing: i32,
    pool: Address,
}


#[pyclass]
pub struct UniswapFetcher {
    provider: Arc<Provider<Http>>,
    block_cache: Arc<Mutex<HashMap<u64, u64>>>,
    token_info_cache: Arc<Mutex<HashMap<Address, (String, String, Number)>>>,
}

#[pymethods]
impl UniswapFetcher {
    #[new]
    fn new(rpc_url: String) -> Self {
        let provider: Arc<Provider<Http>> = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let block_cache: Arc<Mutex<HashMap<u64, u64>>> = Arc::new(Mutex::new(HashMap::new()));
        let token_info_cache: Arc<Mutex<HashMap<Address, (String, String, Number)>>> = Arc::new(Mutex::new(HashMap::new()));
        UniswapFetcher { provider, block_cache, token_info_cache }
    }

    fn get_pool_events_by_token_pairs(&self, py: Python, token_pairs: Vec<(String, String, u32)> , from_block: u64, to_block: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_pool_events_by_token_pairs(self.provider.clone(), self.block_cache.clone(), token_pairs, U64::from(from_block), U64::from(to_block))) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_pool_events_by_pool_addresses(&self, py: Python, pool_addresses: Vec<String>, from_block: u64, to_block: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_pool_events_by_pool_addresses(self.provider.clone(), self.block_cache.clone(), pool_addresses.iter().map(|address| Address::from_str(address).unwrap()).collect(), U64::from(from_block), U64::from(to_block))) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_signals_by_pool_address(&self, py: Python, pool_address: String, timestamp: u64, interval: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_signals_by_pool_address(self.provider.clone(), Address::from_str(&pool_address).unwrap(), timestamp, interval)) {
            Ok(result) => Ok(PyValue(result).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_block_number_range(&self, _py: Python, start_timestamp: u64, end_timestamp: u64) -> (u64, u64) {
        let rt = Runtime::new().unwrap();
        let result = rt.block_on(get_block_number_range(self.provider.clone(), start_timestamp, end_timestamp)).unwrap();
        (result.0.as_u64(), result.1.as_u64())
    }

    fn fetch_pool_data(&self, py: Python, token_pairs: Vec<(String, String, u32)>, start_timestamp: u64, end_timestamp: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(fetch_pool_data(self.provider.clone(), self.block_cache.clone(), token_pairs, start_timestamp, end_timestamp)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => return Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_pool_created_events_between_two_timestamps(&self, py: Python, start_timestamp: u64, end_timestamp: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_pool_created_events_between_two_timestamps(self.provider.clone(), self.token_info_cache.clone(), Address::from_str(FACTORY_ADDRESS).unwrap(), start_timestamp, end_timestamp)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_all_tokens(&self, py: Python, start_timestamp: u64, end_timestamp: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_all_tokens(self.provider.clone(), start_timestamp, end_timestamp)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_all_token_pairs(&self, py: Python, start_timestamp: u64, end_timestamp: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_all_token_pairs(self.provider.clone(), start_timestamp, end_timestamp)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_recent_pool_events(&self, py: Python, pool_address: String, start_timestamp: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_recent_pool_events(self.provider.clone(), Address::from_str(&pool_address).unwrap(), start_timestamp)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_timestamp_by_block_number(&self, py: Python, block_number: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_timestamp_by_block_number(self.provider.clone(), block_number)) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }

    fn get_pool_price_ratios(&self, py: Python, pool_address: String, start_timestamp: u64, end_timestamp: u64, interval: u64) -> PyResult<PyObject> {
        let rt = Runtime::new().unwrap();
        match rt.block_on(get_pool_price_ratios(self.provider.clone(), Address::from_str(&pool_address).unwrap(), start_timestamp, end_timestamp, interval, self.block_cache.clone())) {
            Ok(result) => Ok(PyValue(serde_json::json!(result)).into_py(py)),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    }
    
}

fn get_pool_abi() -> Abi {
    let abi_json = include_str!("contracts/uniswap_pool_abi.json");
    serde_json::from_str(abi_json).unwrap()
}

fn get_token_abis() -> Vec<(String, Abi)> {
    let erc20_abi: Abi = serde_json::from_str(include_str!("contracts/erc20_abi.json")).unwrap();
    let erc721_abi: Abi = serde_json::from_str(include_str!("contracts/erc721_abi.json")).unwrap();
    let dstoken_abi: Abi = serde_json::from_str(include_str!("contracts/dstoken_abi.json")).unwrap();
    vec![("erc20".to_string(), erc20_abi), ("erc721".to_string(), erc721_abi), ("dstoken".to_string(), dstoken_abi)]
}


async fn get_pool_address(provider: Arc<Provider<Http>>, factory_address: Address, token0: Address, token1: Address, fee: u32) -> Result<Address, Box<dyn std::error::Error + Send + Sync>> {
    // Load the Uniswap V3 factory ABI
    let abi_json = include_str!("contracts/uniswap_pool_factory_abi.json");
    let abi: Abi = serde_json::from_str(abi_json)?;

    // Instantiate the contract
    let factory = Contract::new(factory_address, abi, provider.clone());

    // Call the getPool function
    let pool_address: Address = factory.method("getPool", (token0, token1, U256::from(fee)))?.call().await?;

    Ok(pool_address)
}


async fn get_pool_events_by_pool_addresses(
    provider: Arc<Provider<Http>>,
    block_cache: Arc<Mutex<HashMap<u64, u64>>>,
    pool_addresses: Vec<H160>,
    from_block: U64,
    to_block: U64
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut current_block_number = from_block;
    let mut logs = Vec::new();
    while current_block_number <= to_block {
        let next_block_number = min(current_block_number + BATCH_SIZE, to_block);
        let filter = Filter::new()
            .address(pool_addresses.clone())
            .from_block(current_block_number)
            .to_block(next_block_number)
            .topic0(vec![
                H256::from_str(SWAP_EVENT_SIGNATURE).unwrap(),
                H256::from_str(MINT_EVENT_SIGNATURE).unwrap(),
                H256::from_str(BURN_EVENT_SIGNATURE).unwrap(),
                H256::from_str(COLLECT_EVENT_SIGNATURE).unwrap(),
            ]);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }
    println!("fetched pool events from_block: {:?}, to_block: {:?}", from_block, to_block);
    let events = serialize_logs(logs, provider.clone(), block_cache.clone()).await?;
    Ok(events)
}

async fn get_pool_events_by_token_pairs(
    provider: Arc<Provider<Http>>,
    block_cache: Arc<Mutex<HashMap<u64, u64>>>,
    token_pairs: Vec<(String, String, u32)>,
    from_block: U64,
    to_block: U64,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {

    // Get the Uniswap V3 factory address
    let factory_address = Address::from_str("0x1F98431c8aD98523631AE4a59f267346ea31F984")?;

    let futures = token_pairs.into_iter().map(|(token0, token1, fee)| {
        let provider = provider.clone();
        async move {
            let token0_address = Address::from_str(&token0)?;
            let token1_address = Address::from_str(&token1)?;
            let pool_address = get_pool_address(provider.clone(), factory_address, token0_address, token1_address, fee).await?;
            Ok(pool_address) as Result<Address, Box<dyn std::error::Error + Send + Sync>>
        }
    });

    let pool_addresses_results = join_all(futures).await;

    let mut pool_addresses = Vec::new();
    for result in pool_addresses_results {
        match result {
            Ok(pool_address) => pool_addresses.push(pool_address),
            Err(e) => return Err(e),
        }
    }

    println!("Fetched pool address: {:?}", pool_addresses);

    let events = get_pool_events_by_pool_addresses(provider.clone(), block_cache.clone(), pool_addresses, from_block, to_block).await?;
    Ok(events)
    
}

async fn serialize_logs(logs: Vec<Log>, provider: Arc::<Provider<Http>>, block_cache: Arc<Mutex<HashMap<u64, u64>>>) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut data = Vec::new();
    for log in logs {
        match decode_uniswap_event(&log) {
            Ok(event) => {
                let (uniswap_event, transaction_hash, block_number) = event;
                let timestamp = {
                    let mut cache = block_cache.lock().await;
                    if let Some(&cached_timestamp) = cache.get(&block_number) {
                        cached_timestamp
                    } else {
                        let block = provider.get_block(block_number).await?.ok_or("Block not found")?;
                        let timestamp = block.timestamp.as_u64();
                        cache.insert(block_number, timestamp);
                        timestamp
                    }
                };
                let mut uniswap_event_with_metadata = match uniswap_event {
                    UniswapEvent::Swap(event) => serde_json::json!({ "event": { "type": "swap", "data": event } }),
                    UniswapEvent::Mint(event) => serde_json::json!({ "event": { "type": "mint", "data": event } }),
                    UniswapEvent::Burn(event) => serde_json::json!({ "event": { "type": "burn", "data": event } }),
                    UniswapEvent::Collect(event) => serde_json::json!({ "event": { "type": "collect", "data": event } }),
                };
                uniswap_event_with_metadata.as_object_mut().unwrap().insert("transaction_hash".to_string(), serde_json::Value::String(hex::encode(transaction_hash.as_bytes())));
                uniswap_event_with_metadata.as_object_mut().unwrap().insert("block_number".to_string(), serde_json::Value::Number(serde_json::Number::from(block_number)));
                uniswap_event_with_metadata.as_object_mut().unwrap().insert("timestamp".to_string(), serde_json::Value::Number(serde_json::Number::from(timestamp)));
                uniswap_event_with_metadata.as_object_mut().unwrap().insert("pool_address".to_string(), serde_json::Value::String(format!("{:?}", log.address)));
                data.push(uniswap_event_with_metadata);
            },
            Err(e) => return Err(e),
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_string(&data)?);
    let overall_data_hash = format!("{:x}", hasher.finalize());
    Ok(serde_json::json!({ "data": data, "overall_data_hash": overall_data_hash }))
}

async fn get_block_number_range(provider:Arc::<Provider<Http>>, start_timestamp: u64 , end_timestamp: u64) -> Result<(U64, U64), Box<dyn std::error::Error + Send + Sync>>{
    
    // Check if the given date time is more than the current date time
    let current_timestamp = Utc::now().timestamp() as u64;
    if start_timestamp > current_timestamp || end_timestamp > current_timestamp {
        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Given date time is in the future")));
    }

    // let block_number = provider.get_block_number().await?;
    let average_block_time = get_average_block_time(provider.clone()).await?;

    let mut start_block_number = get_block_number_from_timestamp(provider.clone(), start_timestamp, average_block_time).await?;
    let mut start_block_timestamp = provider.get_block(start_block_number).await?.ok_or("Block not found")?.timestamp.as_u64();
    while start_block_timestamp < start_timestamp {
        start_block_number = start_block_number + 1;
        start_block_timestamp = provider.get_block(start_block_number).await?.ok_or("Block not found")?.timestamp.as_u64();
    }
    let mut end_block_number = get_block_number_from_timestamp(provider.clone(), end_timestamp, average_block_time).await?;
    let mut end_block_timestamp = provider.get_block(end_block_number).await?.ok_or("Block not found")?.timestamp.as_u64();
    while end_block_timestamp > end_timestamp {
        end_block_number = end_block_number - 1;
        end_block_timestamp = provider.get_block(end_block_number).await?.ok_or("Block not found")?.timestamp.as_u64();
    }

    Ok((start_block_number, end_block_number))
}

async fn get_average_block_time(provider: Arc<Provider<Http>>) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    // Fetch the latest block
    let latest_block: Block<H256> = provider.get_block(BlockNumber::Latest).await?.ok_or("Latest block not found")?;
    let latest_block_number = latest_block.number.ok_or("Latest block number not found")?;

    // Create a vector of tasks to fetch block timestamps concurrently
    let mut tasks = Vec::new();
    for i in 0..NUM_BLOCKS {
        let provider = provider.clone();
        let block_number = latest_block_number - U64::from(i);
        tasks.push(tokio::spawn(async move {
            let block: Block<H256> = provider.get_block(block_number).await?.ok_or("Block not found")?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(block.timestamp.as_u64())
        }));
    }

    // Collect the results
    let mut timestamps = Vec::new();
    for task in tasks {
        timestamps.push(task.await??);
    }

    // Calculate the time differences between consecutive blocks
    let mut time_diffs = Vec::new();
    for i in 1..timestamps.len() {
        time_diffs.push(timestamps[i - 1] - timestamps[i]);
    }

    // Compute the average block time
    let total_time_diff: u64 = time_diffs.iter().sum();
    let average_block_time = total_time_diff / time_diffs.len() as u64;

    Ok(average_block_time)
}

async fn get_block_number_from_timestamp(
    provider: Arc<Provider<Http>>,
    timestamp: u64,
    average_block_time: u64
) -> Result<U64, Box<dyn std::error::Error + Send + Sync>> {
    // Fetch the latest block
    let latest_block: Block<H256> = provider.get_block(BlockNumber::Latest).await?.ok_or("Latest block not found")?;
    let latest_block_number = latest_block.number.ok_or("Latest block number not found")?;
    let latest_block_timestamp = latest_block.timestamp.as_u64();

    // Estimate the block number using the average block time
    let mut timestamp = timestamp;
    if timestamp > latest_block_timestamp {
        timestamp = latest_block_timestamp;
    }
    let estimated_block_number = latest_block_number.as_u64() - (latest_block_timestamp - timestamp) / average_block_time;

    // Perform exponential search to find the range
    let mut low = U64::zero();
    let mut high = latest_block_number;
    let mut mid = U64::from(estimated_block_number);

    while low < high {
        let block: Block<H256> = provider.get_block(mid).await?.ok_or("Block not found")?;
        let block_timestamp = block.timestamp.as_u64();

        if block_timestamp < timestamp {
            low = mid + 1;
        } else {
            high = mid;
        }

        // Adjust mid for exponential search
        mid = (low + high) / 2;
    }

    Ok(low)
}

async fn fetch_pool_data(provider: Arc::<Provider<Http>>, block_cache: Arc<Mutex<HashMap<u64, u64>>>, token_pairs: Vec<(String, String, u32)>, start_timestamp: u64, end_timestamp: u64) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    // let date_str = "2024-09-27 19:34:56";
    let (from_block, to_block) = get_block_number_range(provider.clone(), start_timestamp, end_timestamp).await?;
    let pool_events = get_pool_events_by_token_pairs(provider.clone(), block_cache.clone(), token_pairs, from_block, to_block,).await?;
    Ok(pool_events)
}

async fn get_pool_created_events_between_two_timestamps(
    provider: Arc<Provider<Http>>,
    token_info_cache: Arc<Mutex<HashMap<Address, (String, String, Number)>>>,
    factory_address: Address,
    start_timestamp: u64,
    end_timestamp: u64,
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    println!("{} | Fetching pool created events between two timestamps", Utc::now());
    let (start_block_number, end_block_number) = get_block_number_range(provider.clone(), start_timestamp, end_timestamp).await?;
    let mut current_block_number = start_block_number;
    let mut logs = Vec::new();
    let erc20_abi: Abi = serde_json::from_str(include_str!("contracts/erc20_abi.json"))?;
    let erc721_abi: Abi = serde_json::from_str(include_str!("contracts/erc721_abi.json"))?;
    let dstoken_abi: Abi = serde_json::from_str(include_str!("contracts/dstoken_abi.json"))?;
    let abis: Vec<(String, Abi)> = vec![("erc20".to_string(), erc20_abi), ("erc721".to_string(), erc721_abi), ("dstoken".to_string(), dstoken_abi)];

    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE as u64, end_block_number);
        let filter = Filter::new()
            .address(factory_address)
            .topic0(H256::from_str(POOL_CREATED_SIGNATURE).unwrap())
            .from_block(current_block_number)
            .to_block(next_block_number);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }

    let mut pool_created_events = Vec::new();
    for log in logs {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.to_vec(),
        };

        if log.topics[0] == H256::from_str(POOL_CREATED_SIGNATURE).unwrap() {
            let pool_created_event = <PoolCreatedEvent as EthLogDecode>::decode_log(&raw_log)?;
            let token0_info = {
                let mut cache = token_info_cache.lock().await;
                if let Some(cached_token_info) = cache.get(&pool_created_event.token0) {
                    cached_token_info.clone()
                } else {
                    let token_info = get_token_info(provider.clone(), pool_created_event.token0, abis.clone()).await.unwrap_or_else(|_| ("".to_string(), "".to_string(), 0.into()));
                    cache.insert(pool_created_event.token0, token_info.clone());
                    token_info
                }
            };
            let token1_info = {
                let mut cache = token_info_cache.lock().await;
                if let Some(cached_token_info) = cache.get(&pool_created_event.token1) {
                    cached_token_info.clone()
                } else {
                    let token_info = get_token_info(provider.clone(), pool_created_event.token1, abis.clone()).await.unwrap_or_else(|_| ("".to_string(), "".to_string(), 0.into()));
                    cache.insert(pool_created_event.token1, token_info.clone());
                    token_info
                }
            };
            pool_created_events.push(serde_json::json!({
                "token0": {
                    "address": pool_created_event.token0,
                    "name": token0_info.0,
                    "symbol": token0_info.1,
                    "decimals": token0_info.2,
                },
                "token1": {
                    "address": pool_created_event.token1,
                    "name": token1_info.0,
                    "symbol": token1_info.1,
                    "decimals": token1_info.2,
                },
                "fee": pool_created_event.fee,
                "tick_spacing": pool_created_event.tick_spacing,
                "pool_address": pool_created_event.pool,
                "block_number": log.block_number.unwrap().as_u64(),
            }));
        }
    }
    println!("{} | Completed fetching pool created events", Utc::now());
    Ok(pool_created_events)
}

async fn get_signals_by_pool_address(
    provider: Arc<Provider<Http>>,
    pool_address: Address,
    timestamp: u64,
    interval: u64,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let average_block_time = get_average_block_time(provider.clone()).await?;
    let start_block_number = get_block_number_from_timestamp(provider.clone(), timestamp, average_block_time).await?;
    let end_block_number = start_block_number + interval as u64;
    let mut current_block_number = start_block_number;
    let mut logs = Vec::new();
    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE as u64, end_block_number);
        let filter = Filter::new()
            .address(pool_address)
            .from_block(current_block_number)
            .to_block(next_block_number)
            .topic0(vec![
                H256::from_str(SWAP_EVENT_SIGNATURE).unwrap(),
                H256::from_str(MINT_EVENT_SIGNATURE).unwrap(),
                H256::from_str(BURN_EVENT_SIGNATURE).unwrap(),
            ]);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }
    let events = serialize_logs(logs, provider.clone(), Arc::new(Mutex::new(HashMap::new()))).await?;
    let data = events["data"].as_array().unwrap();
    let mut price: f64 = 0.0;
    let mut volume: I256 = I256::from(0);
    let mut liquidity: BigInt = BigInt::from(0);
    let mut swap_event_count: i32 = 0;
    
    for event in data {
        let event_type = event["event"]["type"].as_str().unwrap();
        let event_data = event["event"]["data"].clone();
        match event_type {
            "swap" => {
                let swap_event: SwapEvent = serde_json::from_value(event_data)?;
                let sqrt_price = ( swap_event.sqrt_price_x96 / 2u128.pow(96) ).as_u128() as f64;
                price = price + sqrt_price * sqrt_price;
                swap_event_count = swap_event_count + 1;
                volume = volume + swap_event.amount0.abs() + swap_event.amount1.abs();
            },
            "mint" => {
                let mint_event: MintEvent = serde_json::from_value(event_data)?;
                liquidity = liquidity + BigInt::parse_bytes(mint_event.amount.to_string().as_bytes(), 10).unwrap();
            },
            "burn" => {
                let burn_event: BurnEvent = serde_json::from_value(event_data)?;
                liquidity = liquidity - BigInt::parse_bytes(burn_event.amount.to_string().as_bytes(), 10).unwrap();
            },
            _ => (),
        }
    }
    if swap_event_count > 0 {
        price = price / swap_event_count as f64;
    }
    let signals = serde_json::json!({
        "price": price.to_string(),
        "volume": volume.to_string(),
        "liquidity": liquidity.to_string(),
    });
    Ok(signals)
}

async fn get_token_info(provider: Arc<Provider<Http>>, token_address: Address, abis: Vec<(String, Abi)>) -> Result<(String, String, Number), Box<dyn std::error::Error + Send + Sync>> {
    
    let contracts: Vec<_> = abis.iter().map(|abi| (abi.0.clone(), Contract::new(token_address, abi.1.clone(), provider.clone()))).collect();
    
    for (contract_type, contract) in contracts {
        if contract_type == "erc20" {
            let name: Result<String, _> = contract.method::<(), String>("name", ())?.call().await;
            let symbol: Result<String, _> = contract.method::<(), String>("symbol", ())?.call().await;
            let decimals: Result<u8, _> = contract.method::<(), u8>("decimals", ())?.call().await;
            match (name, symbol, decimals) {
                (Ok(name), Ok(symbol), Ok(decimals)) => return Ok((
                    name.trim_end_matches('\0').to_string(),
                    symbol.trim_end_matches('\0').to_string(),
                    decimals.into())),
                _ => continue,
            }
        } else if contract_type == "erc721" {
            let name: Result<String, _> = contract.method::<(), String>("name", ())?.call().await;
            let symbol: Result<String, _> = contract.method::<(), String>("symbol", ())?.call().await;
            match (name, symbol) {
                (Ok(name), Ok(symbol)) => return Ok((
                    name.trim_end_matches('\0').to_string(),
                    symbol.trim_end_matches('\0').to_string(),
                    1.into())),
                _ => continue,
            }
        } else if contract_type == "dstoken" {
            let name: Result<String, _> = contract.method::<(), [u8; 32]>("name", ())?.call().await.map(|bytes| {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..end]).to_string()
            });
            let symbol: Result<String, _> = contract.method::<(), [u8; 32]>("symbol", ())?.call().await.map(|bytes| {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..end]).to_string()
            });
            let decimals: Result<u8, _> = contract.method::<(), U256>("decimals", ())?.call().await.map(|decimals| decimals.as_u32() as u8);
            match (name, symbol, decimals) {
                (Ok(name), Ok(symbol), Ok(decimals)) => return Ok((name, symbol, decimals.into())),
                _ => continue,
            }
        }
    }
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Token info not found")))
    
}

async fn get_all_token_pairs(
    provider: Arc<Provider<Http>>,
    start_timestamp: u64,
    end_timestamp: u64
) -> Result<Vec<(Address, Address, u32, Address)>, Box<dyn std::error::Error + Send + Sync>> {
    let factory_address = Address::from_str(FACTORY_ADDRESS)?;
    let (start_block_number, end_block_number) = get_block_number_range(provider.clone(), start_timestamp, end_timestamp).await?;
    let mut logs = Vec::new();
    let mut current_block_number = start_block_number;
    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE, end_block_number);
        let filter = Filter::new()
            .address(factory_address)
            .topic0(H256::from_str(POOL_CREATED_SIGNATURE).unwrap())
            .from_block(current_block_number)
            .to_block(next_block_number);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }

    let mut token_pairs = Vec::new();
    for log in logs {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.to_vec(),
        };
        if log.topics[0] == H256::from_str(POOL_CREATED_SIGNATURE).unwrap() {
            let pool_created_event = <PoolCreatedEvent as EthLogDecode>::decode_log(&raw_log)?;
            token_pairs.push((pool_created_event.token0, pool_created_event.token1, pool_created_event.fee, pool_created_event.pool));
        }
    }

    Ok(token_pairs)
}

async fn get_all_tokens(
    provider: Arc<Provider<Http>>,
    start_timestamp: u64,
    end_timestamp: u64
) -> Result<HashSet<Address>, Box<dyn std::error::Error + Send + Sync>> {
    println!("{} | Fetching all tokens between {} and {}", Utc::now(),start_timestamp, end_timestamp);
    let factory_address = Address::from_str(FACTORY_ADDRESS)?;
    let (start_block_number, end_block_number) = get_block_number_range(provider.clone(), start_timestamp, end_timestamp).await?;
    let mut logs = Vec::new();
    let mut current_block_number = start_block_number;
    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE, end_block_number);
        let filter = Filter::new()
            .address(factory_address)
            .topic0(H256::from_str(POOL_CREATED_SIGNATURE).unwrap())
            .from_block(current_block_number)
            .to_block(next_block_number);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }

    let mut token_addresses = HashSet::new();
    for log in logs {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.to_vec(),
        };
        if log.topics[0] == H256::from_str(POOL_CREATED_SIGNATURE).unwrap() {
            let pool_created_event = <PoolCreatedEvent as EthLogDecode>::decode_log(&raw_log)?;
            token_addresses.insert(pool_created_event.token0);
            token_addresses.insert(pool_created_event.token1);
        }
    }
    println!("{} | Fetched {} unique tokens", Utc::now(), token_addresses.len());
    println!("{} | Completed fetching all tokens between {} and {}", Utc::now(), start_timestamp, end_timestamp);
    Ok(token_addresses)
}

async fn get_recent_pool_events(
    provider: Arc<Provider<Http>>,
    pool_address: Address,
    start_timestamp: u64,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    println!("{} | Fetching recent pool events for pool {} starting from timestamp {}", Utc::now(), pool_address, start_timestamp);
    let average_block_time = get_average_block_time(provider.clone()).await?;
    let start_block_number = get_block_number_from_timestamp(provider.clone(), start_timestamp, average_block_time).await?;
    let end_block_number = provider.get_block_number().await?;
    let mut current_block_number = start_block_number;
    let mut logs = Vec::new();
    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE, end_block_number);
        let filter = Filter::new()
            .address(pool_address)
            .from_block(current_block_number)
            .to_block(next_block_number)
            .topic0(vec![
                H256::from_str(SWAP_EVENT_SIGNATURE).unwrap(),
                H256::from_str(MINT_EVENT_SIGNATURE).unwrap(),
                H256::from_str(BURN_EVENT_SIGNATURE).unwrap(),
                H256::from_str(COLLECT_EVENT_SIGNATURE).unwrap(),
            ]);
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }
    let events = serialize_logs(logs, provider.clone(), Arc::new(Mutex::new(HashMap::new()))).await?;
    println!("{} | Completed fetching recent pool events for pool {} starting from timestamp {}", Utc::now(), pool_address, start_timestamp);
    Ok(events)
}

async fn get_timestamp_by_block_number(provider: Arc<Provider<Http>>, block_number: u64) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let block = provider.get_block(U64::from(block_number)).await?.ok_or("Block not found")?;
    Ok(block.timestamp.as_u64())
}

async fn get_pool_info(
    provider: Arc<Provider<Http>>,
    pool_address: Address,
    pool_abi: Abi,
) -> Result<(Address, Address, u32, i32), Box<dyn std::error::Error + Send + Sync>> {
    let pool_contract = Contract::new(pool_address, pool_abi.clone(), provider.clone());
    let token0: Address = pool_contract.method::<(), Address>("token0", ())?.call().await?;
    let token1: Address = pool_contract.method::<(), Address>("token1", ())?.call().await?;
    let fee: u32 = pool_contract.method::<(), u32>("fee", ())?.call().await?;
    let tick_spacing: i32 = pool_contract.method::<(), i32>("tickSpacing", ())?.call().await?;
    Ok((token0, token1, fee, tick_spacing))
}

async fn get_pool_price_ratios(
    provider: Arc<Provider<Http>>,
    pool_address: Address,
    start_timestamp: u64,
    end_timestamp: u64,
    interval: u64,
    block_cache: Arc<Mutex<HashMap<u64, u64>>>,
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let (start_block_number, end_block_number) = get_block_number_range(provider.clone(), start_timestamp, end_timestamp).await?;

    let pool_abi = get_pool_abi();
    let token_abis = get_token_abis();

    let (token0, token1, _, _) = get_pool_info(provider.clone(), pool_address, pool_abi.clone()).await?;

    let (_, _, token0_decimals) = get_token_info(provider.clone(), token0, token_abis.clone()).await?;
    let (_, _, token1_decimals) = get_token_info(provider.clone(), token1, token_abis.clone()).await?;
    let mut logs = Vec::new();
    let mut current_block_number = start_block_number;
    while current_block_number <= end_block_number {
        let next_block_number = min(current_block_number + BATCH_SIZE, end_block_number);
        let filter = Filter::new()
            .address(pool_address)
            .from_block(current_block_number)
            .to_block(next_block_number)
            .topic0(H256::from_str(SWAP_EVENT_SIGNATURE).unwrap());
        let block_logs = provider.get_logs(&filter).await?;
        logs.extend(block_logs);
        current_block_number = next_block_number + 1;
    }

    let mut price_ratios = HashMap::new();
    // initialize the price ratios with the timestamps between start_timestamp and end_timestamp
    let mut timestamp = (start_timestamp + interval) / interval * interval;
    while timestamp <= end_timestamp {
        price_ratios.insert(timestamp, 0.0);
        timestamp = timestamp + interval;
    }
    for log in logs {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.to_vec(),
        };
        let timestamp = {
            let mut cache = block_cache.lock().await;
            if let Some(&cached_timestamp) = cache.get(&log.block_number.unwrap().as_u64()) {
                cached_timestamp
            } else {
                let block = provider.get_block(log.block_number.unwrap()).await?.ok_or("Block not found")?;
                let timestamp = block.timestamp.as_u64();
                cache.insert(log.block_number.unwrap().as_u64(), timestamp);
                timestamp
            }
        };
        let aggregated_timestamp = (timestamp + interval) / interval * interval;
        if log.topics[0] == H256::from_str(SWAP_EVENT_SIGNATURE).unwrap() {
            let swap_event = <SwapEvent as EthLogDecode>::decode_log(&raw_log)?;
            let sqrt_price = ( swap_event.sqrt_price_x96 / 2u128.pow(96) ).as_u128() as f64;
            let token0_decimals = token0_decimals.as_u64().unwrap();
            let token1_decimals = token1_decimals.as_u64().unwrap();
            let price_ratio = sqrt_price * sqrt_price * 10u128.pow(token0_decimals as u32) as f64 / 10u128.pow(token1_decimals as u32) as f64;
            
            price_ratios.insert(aggregated_timestamp, price_ratio);
        }
    }
    let mut result: Vec<Value> = price_ratios.iter().map(|(timestamp, price_ratio)| {
        serde_json::json!({
            "timestamp": timestamp,
            "price_ratio": price_ratio,
        })
    }).collect();
    result.sort_by(|a, b| a["timestamp"].as_u64().cmp(&b["timestamp"].as_u64()));
    let mut current_price_ratio = 0.0;
    if result.len() > 0 {
        if result[0]["price_ratio"].as_f64().unwrap() == 0.0 {
            let filter = Filter::new()
            .address(pool_address)
            .from_block(start_block_number - BATCH_SIZE)
            .to_block(start_block_number)
            .topic0(H256::from_str(SWAP_EVENT_SIGNATURE).unwrap());
            let block_logs = provider.get_logs(&filter).await?;
            for block_log in block_logs {
                let raw_log = RawLog {
                    topics: block_log.topics.clone(),
                    data: block_log.data.to_vec(),
                };
                if block_log.topics[0] == H256::from_str(SWAP_EVENT_SIGNATURE).unwrap() {
                    let swap_event = <SwapEvent as EthLogDecode>::decode_log(&raw_log)?;
                    let sqrt_price = ( swap_event.sqrt_price_x96 / 2u128.pow(96) ).as_u128() as f64;
                    let token0_decimals = token0_decimals.as_u64().unwrap();
                    let token1_decimals = token1_decimals.as_u64().unwrap();
                    let price_ratio = sqrt_price * sqrt_price * 10u128.pow(token0_decimals as u32) as f64 / 10u128.pow(token1_decimals as u32) as f64;
                    current_price_ratio = price_ratio;
                    break;
                }
            }
        }
    }
    for item in result.iter_mut() {
        if item["price_ratio"].as_f64().unwrap() == 0.0 {
            item.as_object_mut().unwrap().insert("price_ratio".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(current_price_ratio).unwrap()));
        }
        current_price_ratio = item["price_ratio"].as_f64().unwrap();
    }
    // change price_ratio type to string
    for item in result.iter_mut() {
        let price_ratio = item["price_ratio"].as_f64().unwrap().to_string();
        item.as_object_mut().unwrap().insert("price_ratio".to_string(), serde_json::Value::String(price_ratio));
    }
    Ok(result)

}


#[pymodule]
fn uniswap_fetcher_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<UniswapFetcher>()?;
    Ok(())
}

// implement test logic
#[cfg(test)]
mod tests {

    use super::*;
    use chrono::{NaiveDateTime, Utc, TimeZone};

    #[tokio::test]
    async fn test_fetch_pool_data() {
        let token0 = "0xaea46a60368a7bd060eec7df8cba43b7ef41ad85";
        let token1 = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let start_datetime = "2021-05-06 00:00:00";
        let end_datetime = "2021-05-07 00:00:00";
        let rpc_url = "http://localhost:8545";
        let fee = 3000;

        let first_naive_datetime = NaiveDateTime::parse_from_str(start_datetime, "%Y-%m-%d %H:%M:%S")
            .expect("Failed to parse date");
        let first_datetime_utc = Utc.from_utc_datetime(&first_naive_datetime);
        let first_timestamp = first_datetime_utc.timestamp() as u64;

        let second_naive_datetime = NaiveDateTime::parse_from_str(end_datetime, "%Y-%m-%d %H:%M:%S")
            .expect("Failed to parse date");
        let second_datetime_utc = Utc.from_utc_datetime(&second_naive_datetime);
        let second_timestamp = second_datetime_utc.timestamp() as u64;
        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let block_cache = Arc::new(Mutex::new(HashMap::new()));
        let token_pairs = vec![(token0.to_string(), token1.to_string(), fee)];

        let result = fetch_pool_data(provider, block_cache, token_pairs, first_timestamp, second_timestamp).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_pool_events_by_token_pairs() {
        let token0 = "0xaea46a60368a7bd060eec7df8cba43b7ef41ad85";
        let token1 = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let from_block = 12345678;
        let to_block = 12345778;
        let rpc_url = "http://localhost:8545";
        let fee = 3000;

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let block_cache = Arc::new(Mutex::new(HashMap::new()));
        let token_pairs = vec![(token0.to_string(), token1.to_string(), fee)];

        let result = get_pool_events_by_token_pairs(provider, block_cache, token_pairs, U64::from(from_block), U64::from(to_block)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_pool_events_by_pool_addresses() {
        let pool_addresses = vec!["0x11b815efb8f581194ae79006d24e0d814b7697f6"];
        let from_block = 12376933;
        let to_block = 12376933;
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let block_cache = Arc::new(Mutex::new(HashMap::new()));
        let pool_addresses: Vec<Address> = pool_addresses.iter().map(|address| Address::from_str(address).unwrap()).collect();

        let result = get_pool_events_by_pool_addresses(provider, block_cache, pool_addresses, U64::from(from_block), U64::from(to_block)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_signals_by_pool_address() {
        let pool_address = "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8";
        let timestamp = 1613015400; // 2021-10-01 00:00:00 UTC
        let interval = 300; // 5-min in seconds
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let pool_address = Address::from_str(pool_address).unwrap();

        let result = get_signals_by_pool_address(provider, pool_address, timestamp, interval).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_block_number_range() {
        let start_timestamp = 1620086400; // 2021-10-01 00:00:00 UTC
        let end_timestamp = 1620172800; // 2021-10-02 00:00:00 UTC
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());

        let result = get_block_number_range(provider, start_timestamp, end_timestamp).await;
        assert!(result.is_ok());
        let (start_block_number, end_block_number) = result.unwrap();
        dbg!(start_block_number, end_block_number);
    }

    #[tokio::test]
    async fn test_get_pool_created_events_between_two_timestamps() {
        let start_timestamp = 1633046400; // 2021-10-01 00:00:00 UTC
        let end_timestamp = 1633132800; // 2021-10-02 00:00:00 UTC
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let factory_address = Address::from_str(FACTORY_ADDRESS).unwrap();
        let token_info_cache = Arc::new(Mutex::new(HashMap::new()));

        let result = get_pool_created_events_between_two_timestamps(provider, token_info_cache.clone(), factory_address, start_timestamp, end_timestamp).await;
        assert!(result.is_ok());
    }
    #[tokio::test]
    async fn test_get_token_info() {
        let token_address = "0x0e84296da31b6c475afc1a991db05e79633e67b0";
        let rpc_url = "http://localhost:8545";
        let erc20_abi_json = include_str!("contracts/erc20_abi.json");
        let erc721_abi_json = include_str!("contracts/erc721_abi.json");
        let dstoken_abi_json = include_str!("contracts/dstoken_abi.json");
        let erc20_abi: Abi = serde_json::from_str(erc20_abi_json).unwrap();
        let erc721_abi: Abi = serde_json::from_str(erc721_abi_json).unwrap();
        let dstoken_abi: Abi = serde_json::from_str(dstoken_abi_json).unwrap();
        let abis: Vec<(String, Abi)> = vec![("erc20".to_string(), erc20_abi), ("erc721".to_string(), erc721_abi), ("dstoken".to_string(), dstoken_abi)];
        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());

        let result = get_token_info(provider.clone(), Address::from_str(token_address).unwrap(), abis.clone()).await;
        assert!(result.is_ok());
        let token_info = result.unwrap();
        dbg!(token_info);
    }

    #[tokio::test]
    async fn test_get_all_tokens() {
        let start_timestamp = 1633046400; // 2021-10-01 00:00:00 UTC
        let end_timestamp = 1635030400; // 2021-10-02 00:00:00 UTC
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());

        let result = get_all_tokens(provider, start_timestamp, end_timestamp).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_recent_pool_events() {
        let pool_address = "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8";
        let timestamp = 1733702400; // 2024-12-08 00:00:00 UTC
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let pool_address = Address::from_str(pool_address).unwrap();

        let result = get_recent_pool_events(provider, pool_address, timestamp).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_all_token_pairs() {
        let start_timestamp = 1633046400; // 2021-10-01 00:00:00 UTC
        let end_timestamp = 1635030400; // 2021-10-02 00:00:00 UTC
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());

        let result = get_all_token_pairs(provider, start_timestamp, end_timestamp).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_timestamp_by_blocknumber() {
        let block_number = 12376933;
        let rpc_url = "http://localhost:8545";

        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());

        let result = get_timestamp_by_block_number(provider, block_number).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_recent_price_ratio() {
        let pool_address = "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8";
        let start_timestamp = 1733875210; // 2021-10-08 00:00:00 UTC
        let end_timestamp = 1733877000; // 2021-10-08 01:00:00 UTC
        let interval = 300; // 5-min in seconds
        let rpc_url = "http://localhost:8545";
        let block_cache = Arc::new(Mutex::new(HashMap::new()));
        
        let provider = Arc::new(Provider::<Http>::try_from(rpc_url).unwrap());
        let pool_address = Address::from_str(pool_address).unwrap();

        let result = get_pool_price_ratios(provider, pool_address, start_timestamp, end_timestamp, interval, block_cache).await;
        assert!(result.is_ok());
        let values = result.unwrap();
        dbg!(values);
    }

}
