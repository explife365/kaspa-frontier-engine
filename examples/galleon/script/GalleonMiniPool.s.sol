// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonMiniPool} from "../src/GalleonMiniPool.sol";

/// @dev Galleon L2 only. Uses gTEST + wiKAS live addresses.
/// forge script script/GalleonMiniPool.s.sol:GalleonMiniPoolScript --rpc-url galleon_testnet --broadcast
contract GalleonMiniPoolScript is Script {
    address internal constant GTEST = 0xBC5e27AB3CE2eDB243593CDa2437E5B30e0D5D7D;
    address internal constant WIKAS = 0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f;

    function run() external {
        vm.startBroadcast();
        new GalleonMiniPool(GTEST, WIKAS);
        vm.stopBroadcast();
    }
}
