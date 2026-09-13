// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {GalleonCoinFlip} from "../src/GalleonCoinFlip.sol";
import {GalleonDice} from "../src/GalleonDice.sol";
import {GalleonJackpot} from "../src/GalleonJackpot.sol";

contract GalleonGamesTest is Test {
    GalleonTestToken internal token;
    address internal house = address(0xC0FFEE);
    address internal player = address(0xBEEF);

    function setUp() public {
        token = new GalleonTestToken(1_000_000 ether);
        token.transfer(player, 1_000 ether);
        token.transfer(house, 1_000 ether);
    }

    function test_coin_flip_pays_winner() public {
        GalleonCoinFlip flip = new GalleonCoinFlip(address(token), house, 1 ether, 100 ether, 196, 100);
        token.transfer(address(flip), 500 ether);
        vm.startPrank(player);
        token.approve(address(flip), 100 ether);
        (bool won, uint256 payout) = flip.flip(true, 10 ether);
        vm.stopPrank();
        if (won) {
            assertEq(payout, 19.6 ether);
        }
        assertEq(flip.gamesPlayed(), 1);
    }

    function test_dice_valid_guess() public {
        GalleonDice dice = new GalleonDice(address(token), house, 1 ether, 50 ether, 45, 10);
        token.transfer(address(dice), 500 ether);
        vm.startPrank(player);
        token.approve(address(dice), 50 ether);
        (uint8 result, uint256 payout) = dice.rollDice(3, 5 ether);
        vm.stopPrank();
        assertGe(result, 1);
        assertLe(result, 6);
        if (result == 3) {
            assertEq(payout, 22.5 ether);
        }
    }

    function test_jackpot_draw() public {
        GalleonJackpot pot = new GalleonJackpot(address(token), house, 1 ether, 60, 500);
        vm.startPrank(player);
        token.approve(address(pot), 10 ether);
        pot.buyTicket();
        vm.stopPrank();
        vm.warp(block.timestamp + 61);
        pot.draw(3600);
        assertEq(pot.roundId(), 1);
        assertEq(pot.ticketCount(), 0);
    }
}
