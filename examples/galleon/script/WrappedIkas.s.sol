// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {WrappedIkas} from "../src/WrappedIkas.sol";

/// @dev Galleon L2 only. Do not put a private key in this file.
/// Simulate first. Igra prepaid must cover gasLimit × gasPrice (2000 gwei floor).
contract WrappedIkasScript is Script {
    function run() external {
        vm.startBroadcast();
        new WrappedIkas();
        vm.stopBroadcast();
    }
}
