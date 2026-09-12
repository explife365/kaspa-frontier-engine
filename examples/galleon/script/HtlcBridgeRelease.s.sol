// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {HtlcBridgeRelease} from "../src/HtlcBridgeRelease.sol";

/// @dev Deploy wiKAS release vault for TN10 HTLC demo (int hashlock 0x48544C43).
contract HtlcBridgeReleaseScript is Script {
    address internal constant WIKAS = 0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f;
    uint256 internal constant PAYMENT_HASH_INT = 0x48544C43;
    uint256 internal constant PAYOUT_WEI = 0.001 ether;

    function run() external {
        vm.startBroadcast();
        new HtlcBridgeRelease(WIKAS, PAYMENT_HASH_INT, PAYOUT_WEI);
        vm.stopBroadcast();
    }
}
