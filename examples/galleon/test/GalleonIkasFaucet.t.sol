// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonIkasFaucet} from "../src/GalleonIkasFaucet.sol";

contract GalleonIkasFaucetTest is Test {
    GalleonIkasFaucet internal faucet;
    address internal user = address(0xBEEF);

    function setUp() public {
        faucet = new GalleonIkasFaucet{value: 1 ether}(0.01 ether, 1 days);
    }

    function test_ownerPushDoesNotNeedUserGas() public {
        vm.deal(user, 0);
        faucet.dripTo(user);
        assertEq(user.balance, 0.01 ether);

        vm.expectRevert(bytes("cooldown"));
        faucet.dripTo(user);

        vm.warp(block.timestamp + 1 days);
        faucet.dripTo(user);
        assertEq(user.balance, 0.02 ether);
    }

    function test_strangerCannotDrip() public {
        vm.prank(user);
        vm.expectRevert(bytes("owner"));
        faucet.dripTo(user);
    }

    function test_emptyReverts() public {
        GalleonIkasFaucet dry = new GalleonIkasFaucet(0.01 ether, 0);
        vm.expectRevert(bytes("empty"));
        dry.dripTo(user);
    }
}
