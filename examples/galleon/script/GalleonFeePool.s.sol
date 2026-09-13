// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonFeePool} from "../src/GalleonFeePool.sol";

/// @dev 30 bps swap fee; 50% of fees to treasury (msg.sender at deploy).
contract GalleonFeePoolScript is Script {
    address internal constant GTEST = 0xBC5e27AB3CE2eDB243593CDa2437E5B30e0D5D7D;
    address internal constant WIKAS = 0x7331B0A33ac9Aa92F506F057bfaA049Ea133f77f;
    uint256 internal constant FEE_BPS = 30;
    uint256 internal constant PROTOCOL_SHARE_BPS = 5_000;

    function run() external {
        vm.startBroadcast();
        new GalleonFeePool(GTEST, WIKAS, msg.sender, FEE_BPS, PROTOCOL_SHARE_BPS);
        vm.stopBroadcast();
    }
}
