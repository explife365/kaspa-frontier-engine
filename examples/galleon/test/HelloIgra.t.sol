// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {HelloIgra} from "../src/HelloIgra.sol";

contract HelloIgraTest is Test {
    HelloIgra internal hello;

    function setUp() public {
        hello = new HelloIgra("hello galleon");
    }

    function test_message() public view {
        assertEq(hello.message(), "hello galleon");
    }

    function test_setMessage() public {
        hello.setMessage("updated");
        assertEq(hello.message(), "updated");
    }
}
