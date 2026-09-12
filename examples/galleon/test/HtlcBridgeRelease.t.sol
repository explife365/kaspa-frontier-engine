// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {HtlcBridgeRelease} from "../src/HtlcBridgeRelease.sol";

contract HtlcBridgeReleaseTest is Test {
    uint256 internal constant PAYMENT_HASH = 0x48544C43;
    uint256 internal constant PAYOUT = 0.01 ether;

    GalleonTestToken internal token;
    HtlcBridgeRelease internal bridge;

    function setUp() public {
        token = new GalleonTestToken(100 ether);
        bridge = new HtlcBridgeRelease(address(token), PAYMENT_HASH, PAYOUT);
        token.approve(address(bridge), type(uint256).max);
        bridge.deposit(PAYOUT);
    }

    function test_claimDemo_paysClaimant() public {
        uint256 before = token.balanceOf(address(this));
        bridge.claimDemo(PAYMENT_HASH);
        assertTrue(bridge.claimed());
        assertEq(bridge.claimant(), address(this));
        assertEq(token.balanceOf(address(this)), before + PAYOUT);
        assertEq(bridge.vaultBalance(), 0);
    }

    function test_claimDemo_revertsWrongPreimage() public {
        vm.expectRevert(bytes("hashlock"));
        bridge.claimDemo(PAYMENT_HASH + 1);
    }

    function test_claimDemo_revertsDoubleClaim() public {
        bridge.claimDemo(PAYMENT_HASH);
        vm.expectRevert(bytes("claimed"));
        bridge.claimDemo(PAYMENT_HASH);
    }

    function test_claimDemo_revertsWhenUnderfunded() public {
        HtlcBridgeRelease empty = new HtlcBridgeRelease(address(token), PAYMENT_HASH, PAYOUT);
        vm.expectRevert(bytes("underfunded"));
        empty.claimDemo(PAYMENT_HASH);
    }
}
