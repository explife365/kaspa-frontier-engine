// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {GalleonCoinFlip} from "../src/GalleonCoinFlip.sol";
import {GalleonDice} from "../src/GalleonDice.sol";
import {GalleonJackpot} from "../src/GalleonJackpot.sol";

/// @dev Deploy all three Galleon testnet games in one broadcast.
contract GalleonGamesScript is Script {
    address internal constant GTEST = 0xBC5e27AB3CE2eDB243593CDa2437E5B30e0D5D7D;

    function run() external {
        vm.startBroadcast();
        new GalleonCoinFlip(GTEST, msg.sender, 0.1 ether, 50 ether, 196, 100);
        new GalleonDice(GTEST, msg.sender, 0.1 ether, 25 ether, 45, 10);
        new GalleonJackpot(GTEST, msg.sender, 0.5 ether, 3600, 500);
        vm.stopBroadcast();
    }
}
