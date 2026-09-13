// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {HtlcBridgeFactory} from "../src/HtlcBridgeFactory.sol";

/// @dev 1% claim fee to treasury (msg.sender at deploy).
contract HtlcBridgeFactoryScript is Script {
    address internal constant WIKAS = 0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f;
    uint256 internal constant CLAIM_FEE_BPS = 100;

    function run() external {
        vm.startBroadcast();
        new HtlcBridgeFactory(WIKAS, msg.sender, CLAIM_FEE_BPS);
        vm.stopBroadcast();
    }
}
