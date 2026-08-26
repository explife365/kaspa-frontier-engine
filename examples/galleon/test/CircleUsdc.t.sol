// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CircleUsdc} from "../src/CircleUsdc.sol";
import {IERC20Runtime} from "../src/IERC20Runtime.sol";

contract CircleUsdcTest is Test {
    function test_ethereumUsdcIsListed() public pure {
        assertEq(CircleUsdc.usdcOnChain(1), CircleUsdc.ETHEREUM);
        assertTrue(CircleUsdc.listsChain(1));
        assertTrue(CircleUsdc.isCircleIssued(1, CircleUsdc.ETHEREUM));
    }

    function test_galleonIsNotACircleChain() public pure {
        assertEq(CircleUsdc.GALLEON_CHAIN_ID, 38836);
        assertEq(CircleUsdc.usdcOnChain(38836), address(0));
        assertFalse(CircleUsdc.listsChain(38836));
        assertFalse(CircleUsdc.isCircleIssued(38836, CircleUsdc.GALLEON_TEST_USDC));
        assertFalse(CircleUsdc.isCircleIssued(1, CircleUsdc.GALLEON_TEST_USDC));
        assertTrue(CircleUsdc.GALLEON_TEST_USDC != CircleUsdc.ETHEREUM);
    }

    function test_baseAndArbitrumNativeUsdc() public pure {
        assertEq(CircleUsdc.usdcOnChain(8453), CircleUsdc.BASE);
        assertEq(CircleUsdc.usdcOnChain(42161), CircleUsdc.ARBITRUM);
        assertTrue(CircleUsdc.isCircleIssued(8453, CircleUsdc.BASE));
        assertFalse(CircleUsdc.isCircleIssued(8453, CircleUsdc.ETHEREUM));
    }

    /// Live Igra test USDC. Skips unless GALLEON_FORK is set (do not fake Circle).
    function testFork_igraTestUsdcIsNotCircle() public {
        string memory url = vm.envOr("GALLEON_FORK", string(""));
        if (bytes(url).length == 0) {
            return;
        }
        vm.createSelectFork(url);
        assertEq(block.chainid, CircleUsdc.GALLEON_CHAIN_ID);
        IERC20Runtime token = IERC20Runtime(CircleUsdc.GALLEON_TEST_USDC);
        assertEq(token.decimals(), 6);
        assertFalse(CircleUsdc.isCircleIssued(block.chainid, address(token)));
        assertTrue(address(token) != CircleUsdc.ETHEREUM);
    }
}
