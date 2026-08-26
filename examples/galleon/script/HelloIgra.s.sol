// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {HelloIgra} from "../src/HelloIgra.sol";

/// @dev Needs Galleon iKAS. Do not put a private key in this file.
///   forge script script/HelloIgra.s.sol:HelloIgraScript --rpc-url galleon_testnet --broadcast --private-key $env:GALLEON_PRIVATE_KEY
contract HelloIgraScript is Script {
    function run() external {
        vm.startBroadcast();
        new HelloIgra("Hello from Igra Galleon");
        vm.stopBroadcast();
    }
}
