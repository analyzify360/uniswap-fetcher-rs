from typing import List, Tuple, Dict, Any

class UniswapFetcher:
    def __init__(self, rpc_url: str) -> None:
        """
        Initialize the UniswapFetcher.

        Args:
            rpc_url (str): The RPC URL of the Ethereum node.
        """
        ...

    def get_pool_events_by_token_pairs(
        self,
        token_pairs: List[Tuple[str, str, int]],
        from_block: int,
        to_block: int
    ) -> Dict:
        """
        Get pool events by token pairs.

        Args:
            token_pairs (List[Tuple[str, str, int]]): List of token pairs and fees.
            from_block (int): Starting block number.
            to_block (int): Ending block number.

        Returns:
            Dict: JSON object containing the pool events.
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
            Dict: JSON object containing the pool data.
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
            interval (int): Interval.

        Returns:
            Dict: JSON object containing the signals.
            ["price": str, "volume": str, "liquidity": str]
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
        """
        ...