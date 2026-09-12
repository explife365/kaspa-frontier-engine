// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {HtlcBridgeReleaseSha256} from "../src/HtlcBridgeReleaseSha256.sol";

/// @dev Deploy wiKAS release vault for TN10 SHA256 HTLC demo (preimage b"HTLC").
contract HtlcBridgeReleaseSha256Script is Script {
    address internal constant WIKAS = 0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f;
    bytes32 internal constant PAYMENT_HASH =
        0x455c7c63032972f9519d91194e1c7facd79fec84e9fcf24ef540f02d85b6e2d4;
    uint256 internal constant PAYOUT_WEI = 0.001 ether;

    function run() external {
        vm.startBroadcast();
        new HtlcBridgeReleaseSha256(WIKAS, PAYMENT_HASH, PAYOUT_WEI);
        vm.stopBroadcast();
    }
}
