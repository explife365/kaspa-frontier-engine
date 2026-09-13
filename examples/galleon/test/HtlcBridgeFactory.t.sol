// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {HtlcBridgeFactory} from "../src/HtlcBridgeFactory.sol";
import {HtlcBridgeVaultSha256} from "../src/HtlcBridgeVaultSha256.sol";

contract HtlcBridgeFactoryTest is Test {
    GalleonTestToken internal wikas;
    HtlcBridgeFactory internal factory;
    address internal treasury = address(0xFEE);
    address internal recipient = address(0xABCD);

    function setUp() public {
        wikas = new GalleonTestToken(1_000 ether);
        factory = new HtlcBridgeFactory(address(wikas), treasury, 100); // 1% fee
    }

    function test_create_and_claim_with_fee() public {
        bytes memory preimage = "tn10-demo-secret";
        bytes32 hash = sha256(preimage);
        uint256 payout = 1 ether;
        uint256 deadline = block.timestamp + 1 days;
        address vaultAddr = factory.createVault(hash, payout, recipient, deadline);
        HtlcBridgeVaultSha256 vault = HtlcBridgeVaultSha256(vaultAddr);
        wikas.approve(vaultAddr, payout);
        vault.deposit(payout);
        (uint256 net, uint256 fee) = vault.quoteClaim();
        assertEq(fee, payout / 100);
        assertEq(net, payout - fee);
        vault.claimSha256(preimage);
        assertEq(wikas.balanceOf(recipient), net);
        assertEq(wikas.balanceOf(treasury), fee);
        assertTrue(vault.claimed());
    }
}
