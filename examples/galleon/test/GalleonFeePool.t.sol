// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {GalleonFeePool} from "../src/GalleonFeePool.sol";

contract GalleonFeePoolTest is Test {
    GalleonTestToken internal token0;
    GalleonTestToken internal token1;
    GalleonFeePool internal pool;
    address internal treasury = address(0xC0FFEE);

    function setUp() public {
        token0 = new GalleonTestToken(10_000 ether);
        token1 = new GalleonTestToken(10_000 ether);
        pool = new GalleonFeePool(address(token0), address(token1), treasury, 30, 5_000);
        token0.approve(address(pool), type(uint256).max);
        token1.approve(address(pool), type(uint256).max);
        pool.addLiquidity(1_000 ether, 2_000 ether);
    }

    function test_swap_charges_fee_and_treasury() public {
        address trader = address(0xBEEF);
        token0.transfer(trader, 100 ether);
        vm.startPrank(trader);
        token0.approve(address(pool), 100 ether);
        uint256 quoted = pool.quoteSwap(100 ether, true);
        pool.swap(100 ether, true, quoted);
        vm.stopPrank();
        uint256 fee = (100 ether * 30) / 10_000;
        uint256 protocolFee = (fee * 5_000) / 10_000;
        assertEq(pool.treasury0(), protocolFee);
        assertGt(quoted, 0);
        assertEq(token1.balanceOf(trader), quoted);
    }

    function test_remove_liquidity() public {
        uint256 shares = pool.balanceOf(address(this));
        uint256 b0Before = token0.balanceOf(address(this));
        uint256 b1Before = token1.balanceOf(address(this));
        pool.removeLiquidity(shares / 2);
        assertGt(token0.balanceOf(address(this)), b0Before);
        assertGt(token1.balanceOf(address(this)), b1Before);
    }

    function test_treasury_withdraw() public {
        address trader = address(0xBEEF);
        token0.transfer(trader, 50 ether);
        vm.startPrank(trader);
        token0.approve(address(pool), 50 ether);
        pool.swap(50 ether, true, 0);
        vm.stopPrank();
        uint256 before = token0.balanceOf(treasury);
        vm.prank(treasury);
        pool.withdrawTreasury(treasury);
        assertGt(token0.balanceOf(treasury), before);
    }
}
