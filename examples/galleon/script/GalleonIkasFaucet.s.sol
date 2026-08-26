// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonIkasFaucet} from "../src/GalleonIkasFaucet.sol";

/// @dev Owner-push faucet. Does not mint iKAS. Do not put a private key here.
contract GalleonIkasFaucetScript is Script {
    function run() external {
        vm.startBroadcast();
        new GalleonIkasFaucet(0.01 ether, 1 days);
        vm.stopBroadcast();
    }
}
