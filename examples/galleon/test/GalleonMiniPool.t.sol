// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {GalleonMiniPool} from "../src/GalleonMiniPool.sol";

contract GalleonMiniPoolTest is Test {
    GalleonTestToken internal token0;
    GalleonTestToken internal token1;
    GalleonMiniPool internal pool;

    function setUp() public {
        token0 = new GalleonTestToken(10_000 ether);
        token1 = new GalleonTestToken(10_000 ether);
        pool = new GalleonMiniPool(address(token0), address(token1));
        token0.approve(address(pool), type(uint256).max);
        token1.approve(address(pool), type(uint256).max);
        pool.addLiquidity(1_000 ether, 2_000 ether);
    }

    function test_reserves() public view {
        (uint256 r0, uint256 r1) = pool.getReserves();
        assertEq(r0, 1_000 ether);
        assertEq(r1, 2_000 ether);
    }

    function test_quoteSwap_zeroForOne() public view {
        uint256 amountIn = 100 ether;
        uint256 out = pool.quoteSwap(amountIn, true);
        uint256 expected = amountIn * 2_000 ether / (1_000 ether + amountIn);
        assertEq(out, expected);
    }

    function test_swap_zeroForOne() public {
        address trader = address(0xBEEF);
        token0.transfer(trader, 100 ether);
        vm.startPrank(trader);
        token0.approve(address(pool), 100 ether);
        uint256 quoted = pool.quoteSwap(100 ether, true);
        pool.swap(100 ether, true, quoted);
        vm.stopPrank();
        (uint256 r0, uint256 r1) = pool.getReserves();
        assertEq(r0, 1_100 ether);
        assertEq(r1, 2_000 ether - quoted);
        assertEq(token1.balanceOf(trader), quoted);
    }

    function test_swap_revertsOnMinOut() public {
        vm.expectRevert(bytes("minOut"));
        pool.swap(10 ether, true, type(uint256).max);
    }
}
