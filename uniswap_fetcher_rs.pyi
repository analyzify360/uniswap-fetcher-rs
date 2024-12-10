from typing import List, Tuple, Dict, Any, Union

class UniswapFetcher:
    def __init__(self, rpc_url: str) -> None:
        """
        Initialize the UniswapFetcher.

        Args:
            rpc_url (str): The RPC URL of the Ethereum node.
        Examples:
        >>> uniswap_fetcher = UniswapFetcher("https://mainnet.infura.io/v3/YOUR_INFURA_PROJECT_ID")
            initialize the UniswapFetcher with the RPC URL "https://mainnet.infura.io/v3/YOUR_INFURA_PROJECT_ID".
        Recommended:
        Use local node for faster response time.
        """
        ...

    def get_pool_events_by_token_pairs(
        self,
        token_pairs: List[Tuple[str, str, int]],
        from_block: int,
        to_block: int
    ) -> Dict[str, Dict[str, Union[str, int, Dict]]]:
        """
        Get pool events by token pairs.

        Args:
            token_pairs (List[Tuple[str, str, int]]): List of token pairs and fees.
            from_block (int): Starting block number.
            to_block (int): Ending block number.

        Returns:
            Dict: JSON object containing the pool events.
            {
                data: [
                    {
                        "event": {
                            "type": str,
                            "data": {
                                SwapEvent, MintEvent, BurnEvent
                            }
                        },
                        "block_number": int,
                        "transaction_hash": str,
                        "pool_address": str,
                        "timestamp": int
                        
                    },
                    ...
                ]
                overall_hash: str
            }
        Examples:
        >>> uniswap_fetcher.get_pool_events_by_token_pairs([("0x6b175474e89094c44da98b954eedeac495", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", 3000)], 10000000, 10000001)
            fetch pool events for the token pair ("0x6b175474e89094c44da98b954", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2") with fee 3000 between the block numbers 10000000 and 10000001.
            
        """
        ...

    def get_block_number_range(
        self,
        start_timestamp: int,
        end_timestamp: int
    ) -> Tuple[int, int]:
        """
        Get block number range for the given timestamp range.

        Args:
            start_timestamp (int): Starting timestamp.
            end_timestamp (int): Ending timestamp.

        Returns:
            Tuple[int, int]: Starting and ending block numbers.
        Examples:
        >>> uniswap_fetcher.get_block_number_range(1620000000, 1620000001)
            fetch block number range between the timestamps 1620000000 and 1620000001.
        """
        ...

    def fetch_pool_data(
        self,
        token_pairs: List[Tuple[str, str, int]],
        start_timestamp: int,
        end_timestamp: int
    ) -> Dict:
        """
        Fetch pool data for the given token pairs within the specified time range.

        Args:
            token_pairs (List[Tuple[str, str, int]]): List of token pairs and fees.
            start_timestamp (int): Starting timstamp.
            end_timestamp (int): Ending timstamp.

        Returns:
            Dict: JSON object containing the pool events.
            {
                data: [
                    {
                        "event": {
                            "type": str,
                            "data": {
                                SwapEvent, MintEvent, BurnEvent
                            }
                        },
                        "block_number": int,
                        "transaction_hash": str,
                        "pool_address": str,
                        "timestamp": int
                        
                    },
                    ...
                ]
                overall_hash: str
            }
        Examples:
        >>> uniswap_fetcher.fetch_pool_data([("0x6b175474e89094c44da98b954eedeac495271d0f", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", 3000)], 1620000000, 1620000001)
            fetch pool data for the token pair ("0x6b175474e89094c44da98b954eedeac495271d0f", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2") with fee 3000 between the timestamps 1620000000 and 1620000001.
        """
        ...

    def get_pool_created_events_between_two_timestamps(
        self,
        start_timestamp: int,
        end_timestamp: int
    ) -> Dict:
        """
        Get pool created events between two timestamps.
        
        Args:
            start_timestamp (int): Starting timestamp.
            end_timestamp (int): Ending timestamp.
        
        Returns:
            Dict: JSON object containing the pool created events.
            [
                {
                    "token0": {"address": str, "name": str, "symbol": str, "decimals": int},
                    "token1": {"address": str, "name": str, "symbol": str, "decimals": int},
                    "pool_address": str, 
                    "block_number": int,
                    "fee": int,
                    "tick_spacing": int
                },
                ...
            ]
        Examples:
        >>> uniswap_fetcher.get_pool_created_events_between_two_timestamps(1620000000, 1620000001)
            fetch pool created events between the timestamps 1620000000 and 1620000001.
        """
        ...
        
    def get_signals_by_pool_address(
        self,
        pool_address: str,
        timestamp: int,
        interval: int
    ) -> Dict:
        """
        Get signals by pool address.

        Args:
            pool_address (str): Pool address.
            timestamp (int): Timestamp.
            interval (int): signal interval in seconds.

        Returns:
            Dict: JSON object containing the signals.
            [{"price": str, "volume": str, "liquidity": str}, ...]
        Examples:
        >>> uniswap_fetcher.
            get_signals_by_pool_address("0x1f98407aaB862CdDeF78Ed252D6f557aA5b0f00d", 1620000000, 3600)
            fetch signals for the pool address "0x1f98407aaB862CdDeF78Ed252D6f557aA5b0f00d" at timestamp 1620000000 with 1 hour interval.
        """
        ...
    
            
    def get_pool_events_by_pool_addresses(
        self,
        pool_addresses: List[str],
        from_block: int,
        to_block: int
    ) -> Dict:
        """
        Get pool events by pool addresses.

        Args:
            pool_addresses (List[str]): List of pool addresses.
            from_block (int): Starting block number.
            to_block (int): Ending block number.

        Returns:
            Dict: JSON object containing the pool events.
            {
                data: [
                    {
                        "event": {
                            "type": str,
                            "data": {
                                SwapEvent, MintEvent, BurnEvent
                            }
                        },
                        "block_number": int,
                        "transaction_hash": str,
                        "pool_address": str,
                        "timestamp": int
                        
                    },
                    ...
                ]
                overall_hash: str
            }
        """
        ...
    
    def get_all_tokens(
        self,
        start_timestamp: int,
        end_timestamp: int
    ) -> List[str]:
        """
        Get all tokens within the specified time range.
        
        Args:
            start_timestamp (int): Starting timestamp.
            end_timestamp (int): Ending timestamp.
        
        Returns:
            List[str]: List of token addresses.
        
        Examples:
        >>> uniswap_fetcher.get_all_tokens(1620000000, 1620000001)
            fetch all tokens between the timestamps 1620000000 and 1620000001.
            ["0x6b175474e89094c44da98b954eedeac495271d0f", ...]
        """
    def get_all_token_pairs(
        self,
        start_timestamp: int,
        end_timestamp: int
    ) -> List[Tuple[str, str, int, str]]:
        """
        Get all token pairs within the specified time range.
        
        Args:
            start_timestamp (int): Starting timestamp.
            end_timestamp (int): Ending timestamp.
        
        Returns:
            List[Tuple[str, str]]: List of token pairs.
        
        Examples:
        >>> uniswap_fetcher.get_all_token_pairs(1620000000, 1620000001)
            fetch all token pairs between the timestamps 1620000000 and 1620000001.
            [("0x6b175474e89094c44da98b954eedeac495271d0f", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", 3000, "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8"), ...]
        """
        ...
    
    def get_recent_pool_events(
        self,
        pool_address: str,
        start_timestamp: int,
        end_timestamp: int
    ) -> Dict:
        """
        Get recent pool events by pool address.
        
        Returns:
            Dict: JSON object containing the pool events.
            {
                data: [
                    {
                        "event": {
                            "type": str,
                            "data": {
                                SwapEvent, MintEvent, BurnEvent
                            }
                        },
                        "block_number": int,
                        "transaction_hash": str,
                        "pool_address": str,
                        "timestamp": int
                        
                    },
                    ...
                ]
                overall_hash: str
            }
        """
        ...
    