// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {CircleUsdc} from "../src/CircleUsdc.sol";
import {GalleonTokenBook} from "../src/GalleonTokenBook.sol";

contract GalleonTokenBookTest is Test {
    function test_igraTestUsdcIsNotCircle() public pure {
        address usdc = GalleonTokenBook.igraTestUsdc();
        assertTrue(GalleonTokenBook.isIgraTestUsdc(usdc));
        assertFalse(GalleonTokenBook.isCircleIssued(usdc));
        assertFalse(GalleonTokenBook.isGtest(usdc));
        assertTrue(usdc != CircleUsdc.ETHEREUM);
    }

    function test_gtestIsNotUsdc() public pure {
        assertTrue(GalleonTokenBook.isGtest(GalleonTokenBook.GTEST));
        assertFalse(GalleonTokenBook.isIgraTestUsdc(GalleonTokenBook.GTEST));
        assertFalse(GalleonTokenBook.isCircleIssued(GalleonTokenBook.GTEST));
    }
}
