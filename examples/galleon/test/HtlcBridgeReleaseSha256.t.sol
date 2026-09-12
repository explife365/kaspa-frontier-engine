// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {HtlcBridgeReleaseSha256} from "../src/HtlcBridgeReleaseSha256.sol";

contract HtlcBridgeReleaseSha256Test is Test {
    bytes32 internal constant PAYMENT_HASH =
        0x455c7c63032972f9519d91194e1c7facd79fec84e9fcf24ef540f02d85b6e2d4;
    bytes internal constant PREIMAGE = hex"48544C43";
    uint256 internal constant PAYOUT = 0.01 ether;

    GalleonTestToken internal token;
    HtlcBridgeReleaseSha256 internal bridge;

    function setUp() public {
        token = new GalleonTestToken(100 ether);
        bridge = new HtlcBridgeReleaseSha256(address(token), PAYMENT_HASH, PAYOUT);
        token.approve(address(bridge), type(uint256).max);
        bridge.deposit(PAYOUT);
    }

    function test_claimSha256_paysClaimant() public {
        uint256 before = token.balanceOf(address(this));
        bridge.claimSha256(PREIMAGE);
        assertTrue(bridge.claimed());
        assertEq(bridge.claimant(), address(this));
        assertEq(token.balanceOf(address(this)), before + PAYOUT);
        assertEq(bridge.vaultBalance(), 0);
    }

    function test_claimSha256_revertsWrongPreimage() public {
        vm.expectRevert(bytes("hashlock"));
        bridge.claimSha256(hex"deadbeef");
    }
}
