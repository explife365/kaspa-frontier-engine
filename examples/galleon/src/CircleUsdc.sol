// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Circle native-USDC directory. Not a token. Not cash.
/// Galleon (38836) is omitted: Circle has not listed that chain.
library CircleUsdc {
    address internal constant ETHEREUM = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    address internal constant BASE = 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913;
    address internal constant ARBITRUM = 0xaf88d065e77c8cC2239327C5EDb3A432268e5831;
    address internal constant OPTIMISM = 0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85;
    address internal constant POLYGON = 0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359;
    address internal constant AVALANCHE = 0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E;

    uint256 internal constant GALLEON_CHAIN_ID = 38836;
    address internal constant GALLEON_TEST_USDC = 0xFd89676CBb3D2742c565aFC02986370ef4ba667A;

    function usdcOnChain(uint256 chainId) internal pure returns (address) {
        if (chainId == 1) return ETHEREUM;
        if (chainId == 8453) return BASE;
        if (chainId == 42161) return ARBITRUM;
        if (chainId == 10) return OPTIMISM;
        if (chainId == 137) return POLYGON;
        if (chainId == 43114) return AVALANCHE;
        return address(0);
    }

    function listsChain(uint256 chainId) internal pure returns (bool) {
        return usdcOnChain(chainId) != address(0);
    }

    function isCircleIssued(uint256 chainId, address token) internal pure returns (bool) {
        address listed = usdcOnChain(chainId);
        return listed != address(0) && listed == token;
    }
}
