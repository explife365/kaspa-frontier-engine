// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";

contract GalleonTestTokenTest is Test {
    GalleonTestToken internal token;

    function setUp() public {
        token = new GalleonTestToken(1_000 ether);
    }

    function test_initialSupply() public view {
        assertEq(token.totalSupply(), 1_000 ether);
        assertEq(token.balanceOf(address(this)), 1_000 ether);
        assertEq(token.symbol(), "gTEST");
    }

    function test_transfer() public {
        address bob = address(0xB0B);
        assertTrue(token.transfer(bob, 10 ether));
        assertEq(token.balanceOf(bob), 10 ether);
        assertEq(token.balanceOf(address(this)), 990 ether);
    }

    function test_permitLetsRelayerMoveTokens() public {
        uint256 pk = 0xA11CE;
        address owner = vm.addr(pk);
        assertTrue(token.transfer(owner, 5 ether));
        uint256 deadline = block.timestamp + 1 days;
        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(token.PERMIT_TYPEHASH(), owner, address(this), uint256(5 ether), uint256(0), deadline)
                )
            )
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, digest);
        token.permit(owner, address(this), 5 ether, deadline, v, r, s);
        assertTrue(token.transferFrom(owner, address(0xB0B), 5 ether));
        assertEq(token.balanceOf(address(0xB0B)), 5 ether);
    }
}
