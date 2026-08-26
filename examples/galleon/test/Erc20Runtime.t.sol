// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {IERC20Runtime} from "../src/IERC20Runtime.sol";
import {GalleonTestToken} from "../src/GalleonTestToken.sol";
import {WrappedIkas} from "../src/WrappedIkas.sol";

contract Erc20RuntimeTest is Test {
    receive() external payable {}

    function test_gTestSatisfiesErc20Runtime() public {
        GalleonTestToken token = new GalleonTestToken(100 ether);
        IERC20Runtime erc20 = IERC20Runtime(address(token));
        assertEq(erc20.symbol(), "gTEST");
        assertEq(erc20.decimals(), 18);
        assertEq(erc20.balanceOf(address(this)), 100 ether);
        assertTrue(erc20.transfer(address(0xB0B), 1 ether));
        assertEq(erc20.balanceOf(address(0xB0B)), 1 ether);
    }

    function test_wiKasSatisfiesErc20Runtime() public {
        vm.deal(address(this), 10 ether);
        WrappedIkas wikas = new WrappedIkas();
        wikas.deposit{value: 4 ether}();
        IERC20Runtime erc20 = IERC20Runtime(address(wikas));
        assertEq(erc20.symbol(), "wiKAS");
        assertEq(erc20.decimals(), 18);
        assertEq(erc20.totalSupply(), 4 ether);
        assertTrue(erc20.transfer(address(0xB0B), 1 ether));
        assertEq(erc20.balanceOf(address(0xB0B)), 1 ether);
        assertEq(address(wikas).balance, 4 ether);
    }
}
