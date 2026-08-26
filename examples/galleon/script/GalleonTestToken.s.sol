// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";

/// @dev Do not put a private key in this file.
contract GalleonTestTokenScript is Script {
    function run() external {
        vm.startBroadcast();
        new GalleonTestToken(1_000_000 ether);
        vm.stopBroadcast();
    }
}
