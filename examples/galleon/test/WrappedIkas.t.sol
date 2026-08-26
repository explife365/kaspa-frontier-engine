// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {IERC20Runtime} from "../src/IERC20Runtime.sol";
import {WrappedIkas} from "../src/WrappedIkas.sol";

contract WrappedIkasTest is Test {
    WrappedIkas internal wikas;
    address internal alice = address(0xA11CE);
    address internal bob = address(0xB0B);

    function setUp() public {
        wikas = new WrappedIkas();
        vm.deal(address(this), 50 ether);
        vm.deal(alice, 20 ether);
        vm.deal(bob, 1 ether);
    }

    receive() external payable {}

    function test_metadata() public view {
        assertEq(wikas.name(), "Wrapped iKAS");
        assertEq(wikas.symbol(), "wiKAS");
        assertEq(wikas.decimals(), 18);
        assertEq(wikas.totalSupply(), 0);
    }

    function test_satisfiesErc20Runtime() public {
        IERC20Runtime erc20 = IERC20Runtime(address(wikas));
        wikas.deposit{value: 5 ether}();
        assertEq(erc20.name(), "Wrapped iKAS");
        assertEq(erc20.symbol(), "wiKAS");
        assertEq(erc20.decimals(), 18);
        assertEq(erc20.totalSupply(), 5 ether);
        assertEq(erc20.balanceOf(address(this)), 5 ether);
        assertTrue(erc20.approve(bob, 2 ether));
        assertEq(erc20.allowance(address(this), bob), 2 ether);
        assertTrue(erc20.transfer(alice, 1 ether));
        vm.prank(bob);
        assertTrue(erc20.transferFrom(address(this), alice, 2 ether));
        assertEq(erc20.balanceOf(alice), 3 ether);
        assertEq(erc20.allowance(address(this), bob), 0);
    }

    function test_depositAndWithdraw() public {
        wikas.deposit{value: 2 ether}();
        assertEq(wikas.balanceOf(address(this)), 2 ether);
        assertEq(wikas.totalSupply(), 2 ether);
        wikas.withdraw(1 ether);
        assertEq(wikas.balanceOf(address(this)), 1 ether);
        assertEq(wikas.totalSupply(), 1 ether);
        assertEq(address(wikas).balance, 1 ether);
    }

    function test_receiveDeposits() public {
        (bool ok,) = address(wikas).call{value: 1 ether}("");
        assertTrue(ok);
        assertEq(wikas.balanceOf(address(this)), 1 ether);
        assertEq(wikas.totalSupply(), 1 ether);
    }

    function test_depositZeroIsNoOp() public {
        wikas.deposit{value: 0}();
        assertEq(wikas.balanceOf(address(this)), 0);
        assertEq(wikas.totalSupply(), 0);
    }

    function test_transferWrapped() public {
        wikas.deposit{value: 3 ether}();
        assertTrue(wikas.transfer(bob, 1 ether));
        assertEq(wikas.balanceOf(bob), 1 ether);
        assertEq(wikas.balanceOf(address(this)), 2 ether);
        assertEq(wikas.totalSupply(), 3 ether);
    }

    function test_approveAndTransferFrom() public {
        wikas.deposit{value: 4 ether}();
        assertTrue(wikas.approve(alice, 3 ether));
        vm.prank(alice);
        assertTrue(wikas.transferFrom(address(this), bob, 3 ether));
        assertEq(wikas.balanceOf(bob), 3 ether);
        assertEq(wikas.allowance(address(this), alice), 0);
        assertEq(wikas.balanceOf(address(this)), 1 ether);
    }

    function test_infiniteApprovalDoesNotDecrement() public {
        wikas.deposit{value: 2 ether}();
        assertTrue(wikas.approve(alice, type(uint256).max));
        vm.prank(alice);
        assertTrue(wikas.transferFrom(address(this), bob, 1 ether));
        assertEq(wikas.allowance(address(this), alice), type(uint256).max);
    }

    function test_revertWithdrawOverBalance() public {
        wikas.deposit{value: 1 ether}();
        vm.expectRevert(bytes("balance"));
        wikas.withdraw(1 ether + 1);
    }

    function test_revertTransferOverBalance() public {
        wikas.deposit{value: 1 ether}();
        vm.expectRevert(bytes("balance"));
        wikas.transfer(bob, 2 ether);
    }

    function test_revertTransferToZero() public {
        wikas.deposit{value: 1 ether}();
        vm.expectRevert(bytes("zero"));
        wikas.transfer(address(0), 1);
    }

    function test_revertTransferFromWithoutAllowance() public {
        wikas.deposit{value: 1 ether}();
        vm.prank(alice);
        vm.expectRevert(bytes("allowance"));
        wikas.transferFrom(address(this), bob, 1 ether);
    }

    function test_withdrawToHeavyReceiverReverts() public {
        HeavyReceiver sink = new HeavyReceiver();
        vm.deal(address(sink), 2 ether);
        sink.seed(wikas);
        vm.expectRevert();
        sink.pull(wikas, 1 ether);
        assertEq(wikas.balanceOf(address(sink)), 2 ether);
        assertEq(wikas.totalSupply(), 2 ether);
    }

    function testFuzz_depositWithdrawRoundTrip(uint96 amount) public {
        amount = uint96(bound(amount, 1, 20 ether));
        uint256 beforeBal = address(this).balance;
        wikas.deposit{value: amount}();
        assertEq(wikas.balanceOf(address(this)), amount);
        assertEq(wikas.totalSupply(), amount);
        wikas.withdraw(amount);
        assertEq(wikas.balanceOf(address(this)), 0);
        assertEq(wikas.totalSupply(), 0);
        assertEq(address(this).balance, beforeBal);
    }

    function testFuzz_transferConservesSupply(uint96 depositAmt, uint96 sendAmt) public {
        depositAmt = uint96(bound(depositAmt, 1, 20 ether));
        sendAmt = uint96(bound(sendAmt, 0, depositAmt));
        wikas.deposit{value: depositAmt}();
        wikas.transfer(bob, sendAmt);
        assertEq(wikas.balanceOf(address(this)) + wikas.balanceOf(bob), depositAmt);
        assertEq(wikas.totalSupply(), depositAmt);
        assertEq(address(wikas).balance, depositAmt);
    }
}

contract HeavyReceiver {
    uint256 public n;

    function seed(WrappedIkas wikas) external payable {
        wikas.deposit{value: 2 ether}();
    }

    function pull(WrappedIkas wikas, uint256 amount) external {
        wikas.withdraw(amount);
    }

    receive() external payable {
        n = 1;
    }
}
