// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonMiniPool} from "../src/GalleonMiniPool.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {WrappedIkas} from "../src/WrappedIkas.sol";

/// @dev Seed deployed GalleonMiniPool with gTEST / wiKAS rehearsal liquidity.
contract GalleonMiniPoolSeedScript is Script {
    address internal constant GTEST = 0xBC5e27AB3CE2eDB243593CDa2437E5B30e0D5D7D;
    address payable internal constant WIKAS =
        payable(0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f);

    function run() external {
        address pool = vm.envAddress("GALLEON_MINI_POOL");
        uint256 amount0 = 100 ether;
        uint256 amount1 = 0.04 ether;
        uint256 wrapWei = 0.04 ether;

        vm.startBroadcast();
        WrappedIkas(WIKAS).deposit{value: wrapWei}();
        GalleonTestToken(GTEST).approve(pool, amount0);
        WrappedIkas(WIKAS).approve(pool, amount1);
        GalleonMiniPool(pool).addLiquidity(amount0, amount1);
        vm.stopBroadcast();
    }
}
